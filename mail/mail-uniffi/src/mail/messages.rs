use super::datatypes::{
    AllListActions, AllMessageActions, AttachmentMetadata, Message, MessageActionSheet,
    MobileAction,
};
use super::datatypes::{LabelAsAction, MimeType, MoveAction};
use super::state::MailUserContextPtr;
use super::{ImagePolicy, MailUserSession, Mailbox, RsvpEventServiceProvider};
use crate::core::datatypes::{Id, RemoteId, UnixTimestamp};
use crate::errors::{
    ActionError, AttachmentDataError, AttachmentDataResult, BodyOutputResult, MobileActionsResult,
    ProtonError, VoidActionResult,
};
use crate::mail::datatypes::{LabelAsOutput, Undo};
use crate::mail::mail_scroller::{
    MessageScroller, MessageScrollerLiveQueryCallback, SearchScroller,
    spawn_message_scroller_watcher,
};
use crate::{LiveQueryCallback, WatchHandle, uniffi_async};
use crate::{PaginatorSearchOptions, declare_live_query_tagger};
use itertools::Itertools as _;
use proton_core_api::services::proton::PrivateEmail;
use proton_core_common::models::Label as RealLabel;
use proton_core_common::utils::MapVec;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_common::MailScroller;
use proton_mail_common::MailUserContext;
use proton_mail_common::Unexpected;
use proton_mail_common::datatypes::message_banner::MessageBanner as RealMessageBanner;
use proton_mail_common::datatypes::theme::MailTheme as RealMailTheme;
use proton_mail_common::datatypes::{
    AttachmentMetadata as RealAttachmentMetadata, MobileAction as RealMobileAction,
    ParsedHeaderValue,
};
use proton_mail_common::decrypted_message::{
    BodyOutput as RealBodyOutput, DecryptedMessageBody, ThemeOpts as RealThemeOpts,
    TransformOpts as RealTransformOpts,
};
use proton_mail_common::models::{self, IncomingDefault, Message as RealMessage};
use proton_mail_common::{
    ActionErrorReason as RealActionErrorReason, ProtonMailError as RealProtonMailError,
};
use stash::orm::Model as _;
use std::sync::Arc;
use tracing::warn;

#[derive(uniffi::Object)]
pub struct DecryptedMessage {
    pub(crate) ctx: MailUserContextPtr,
    pub(crate) sender: PrivateEmail,
    pub(crate) body: DecryptedMessageBody,
}

impl DecryptedMessage {
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, RealProtonMailError> {
        self.ctx
            .upgrade()
            .ok_or(RealProtonMailError::Unexpected(Unexpected::Internal))
    }
}

#[uniffi_export]
impl DecryptedMessage {
    /// Gets the message body as an HTML. This does all of the transformations that are
    /// required based on the options and the user settings.
    #[returns(BodyOutputResult)]
    pub async fn body(self: Arc<Self>, opts: TransformOpts) -> Result<BodyOutput, ProtonError> {
        uniffi_async(async move {
            let tether = self.ctx()?.user_stash().connection().await?;
            Ok::<_, RealProtonMailError>(
                self.body
                    .transformed(&self.sender, opts.into(), &tether)
                    .await
                    .into(),
            )
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    /// The full attachment list contained inside the message body.
    ///
    /// Message/Conversation attachments are limited to only 10.
    pub fn attachments(&self) -> Vec<AttachmentMetadata> {
        self.body
            .metadata
            .attachments
            .iter()
            .cloned()
            .map(|a| RealAttachmentMetadata::from(a).into())
            .collect()
    }

    #[must_use]
    pub fn parsed_header_value(&self, key: &str) -> Vec<String> {
        match self.body.parsed_header_value(key) {
            Some(ParsedHeaderValue::Array(arr)) => arr,
            Some(ParsedHeaderValue::String(s)) => vec![s],
            None => vec![],
        }
    }

    #[must_use]
    pub fn mime_type(&self) -> MimeType {
        self.body.mime_type.into()
    }

    #[must_use]
    pub fn get_pgp_subject(&self) -> Option<String> {
        self.body.pgp_subject.clone()
    }

    #[must_use]
    pub fn failed_to_decrypt(&self) -> bool {
        self.body.failed_to_decrypt()
    }

    #[must_use]
    pub fn raw_body(&self) -> String {
        self.body.body.clone()
    }

    #[must_use]
    pub fn raw_headers(&self) -> String {
        self.body.metadata.header.clone()
    }
}

#[uniffi_export]
impl DecryptedMessage {
    #[returns(VoidActionResult)]
    pub async fn unsubscribe_from_newsletter(self: Arc<Self>) -> Result<(), ProtonError> {
        uniffi_async(async move {
            let u = self.body.unsubscribe_from_newsletter()?;
            self.ctx()?
                .queue_action(u)
                .await
                .map_err(RealProtonMailError::from)?;
            Ok::<_, RealProtonMailError>(())
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    #[returns(AttachmentDataResult)]
    pub async fn load_image(
        self: Arc<Self>,
        url: String,
        policy: ImagePolicy,
    ) -> Result<AttachmentData, AttachmentDataError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let att = self.body.load_image(&ctx, &url, policy.into()).await?;

            Ok(AttachmentData {
                data: att.data,
                mime: att.mime,
            })
        })
        .await
    }

    /// Checks if this mail contains an invitation and, if so, returns its
    /// identifier - you can then use this identifier to fetch event details.
    ///
    /// [1] TODO (NGC-57) implement support for offline-mode
    ///     (this function probably will probably not have to be adjusted, but
    ///     I'm leaving a comment so that we know to update the docs above)
    pub async fn identify_rsvp(self: Arc<Self>) -> Option<Arc<RsvpEventServiceProvider>> {
        uniffi_async(async move {
            let ctx = self.ctx()?;

            let rsvp = self
                .body
                .identify_rsvp(&ctx)
                .await
                .map_err(RealProtonMailError::from)?;

            if let Some(rsvp) = rsvp {
                Ok(Some(Arc::new(RsvpEventServiceProvider::new(
                    self.ctx.clone(),
                    rsvp,
                ))))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|err: RealProtonMailError| warn!(?err, "Couldn't identify RSVP"))
        .ok()
        .flatten()
    }
}

#[derive(uniffi::Record)]
pub struct BodyOutput {
    /// The transformed html of the message.
    pub body: String,

    /// Whether or not [`RemoteContent::Strip`] removed a blockquote.
    pub had_blockquote: bool,

    /// How many html tags it has removed.
    pub tags_stripped: u64,

    /// How many UTM tracking params it has removed.
    pub utm_stripped: u64,

    /// How many html tags it has removed.
    pub remote_images_disabled: u64,

    /// How many embedded images it has disabled.
    pub embedded_images_disabled: u64,

    /// The transform opts that were used. All fields are actually Some.
    pub transform_opts: TransformOpts,

    /// This instructs the client on what banners they should show.
    pub body_banners: Vec<MessageBanner>,
}

impl From<RealBodyOutput> for BodyOutput {
    fn from(output: RealBodyOutput) -> Self {
        Self {
            body: output.body,
            had_blockquote: output.had_blockquote,
            tags_stripped: output.tags_stripped,
            utm_stripped: output.utm_stripped,
            remote_images_disabled: output.remote_images_disabled,
            embedded_images_disabled: output.embedded_images_disabled,
            transform_opts: output.transform_opts.into(),
            body_banners: output.body_banners.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct TransformOpts {
    /// Whether should show block quotes or not. Default: true
    #[uniffi(default = true)]
    pub show_block_quote: bool,

    /// Whether should hide remote images or not. Default: defined in mail settings
    #[uniffi(default = None)]
    pub hide_remote_images: Option<bool>,

    /// Whether should hide embedded images or not. Default: defined in mail settings
    #[uniffi(default = None)]
    pub hide_embedded_images: Option<bool>,

    /// Current settings related to the color scheme.
    /// It affects on which CSS style is used in the HTML body of the message
    ///
    /// Default: None
    /// It assumes that the device supports `@media` queries. In that case
    /// passing theme would be irrelevant.
    ///
    #[uniffi(default = None)]
    pub theme: Option<ThemeOpts>,
}

impl From<RealTransformOpts> for TransformOpts {
    fn from(opts: RealTransformOpts) -> Self {
        Self {
            show_block_quote: opts.show_block_quote,
            hide_remote_images: opts.hide_remote_images,
            hide_embedded_images: opts.hide_embedded_images,
            theme: opts.theme.map(Into::into),
        }
    }
}

impl From<TransformOpts> for RealTransformOpts {
    fn from(opts: TransformOpts) -> Self {
        Self {
            show_block_quote: opts.show_block_quote,
            hide_remote_images: opts.hide_remote_images,
            hide_embedded_images: opts.hide_embedded_images,
            theme: opts.theme.map(Into::into),
        }
    }
}

/// Current settings related to the color scheme.
/// It affects on which CSS style is used in the HTML body of the message
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ThemeOpts {
    /// What is the current UI color scheme, provided by the application.
    ///
    pub current_theme: MailTheme,

    /// While using the dark mode, some bodies of messages might be hard to read.
    /// User has an option to override the theme inside of the message (without changing the overall theme).
    ///
    /// Default: No override provided
    ///
    #[uniffi(default = None)]
    pub theme_override: Option<MailTheme>,

    /// Whether the device supports `@media (prefers-color-scheme: dark) {}` or not.
    ///
    /// Default: True - only Android 9 does not support it (so far)
    ///
    #[uniffi(default = true)]
    pub supports_dark_mode_via_media_query: bool,
}

impl From<RealThemeOpts> for ThemeOpts {
    fn from(opts: RealThemeOpts) -> Self {
        Self {
            current_theme: opts.current_theme.into(),
            theme_override: opts.theme_override.map(Into::into),
            supports_dark_mode_via_media_query: opts.supports_dark_mode_via_media_query,
        }
    }
}

impl From<ThemeOpts> for RealThemeOpts {
    fn from(opts: ThemeOpts) -> Self {
        Self {
            current_theme: opts.current_theme.into(),
            theme_override: opts.theme_override.map(Into::into),
            supports_dark_mode_via_media_query: opts.supports_dark_mode_via_media_query,
        }
    }
}
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum MailTheme {
    LightMode,
    DarkMode,
}

impl From<RealMailTheme> for MailTheme {
    fn from(value: RealMailTheme) -> Self {
        match value {
            RealMailTheme::LightMode => Self::LightMode,
            RealMailTheme::DarkMode => Self::DarkMode,
        }
    }
}

impl From<MailTheme> for RealMailTheme {
    fn from(value: MailTheme) -> Self {
        match value {
            MailTheme::LightMode => Self::LightMode,
            MailTheme::DarkMode => Self::DarkMode,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, uniffi::Enum)]
pub enum MessageBanner {
    BlockedSender,

    PhishingAttempt {
        /// Whether the system or the user marked it as phishing.
        auto: bool,
    },

    Spam {
        /// Whether the system or the user marked it as phishing.
        auto: bool,
    },

    Expiry {
        /// The Unix timestamp indicating when the message expires.
        timestamp: UnixTimestamp,
    },

    AutoDelete {
        /// The Unix timestamp indicating when the message will be deleted.
        timestamp: UnixTimestamp,
    },

    UnsubscribeNewsletter {
        already_unsubscribed: bool,
    },

    ScheduledSend {
        /// The Unix timestamp indicating when the message is scheduled to be sent.
        timestamp: UnixTimestamp,
    },

    Snoozed {
        /// The Unix timestamp indicating when the message will reappear.
        timestamp: UnixTimestamp,
    },

    EmbeddedImages,
    RemoteContent,
    UnableToDecrypt,
}

impl From<RealMessageBanner> for MessageBanner {
    fn from(value: RealMessageBanner) -> Self {
        match value {
            RealMessageBanner::BlockedSender => Self::BlockedSender,
            RealMessageBanner::PhishingAttempt { auto } => Self::PhishingAttempt { auto },
            RealMessageBanner::Spam { auto } => Self::Spam { auto },
            RealMessageBanner::Expiry { timestamp } => Self::Expiry {
                timestamp: timestamp.into(),
            },
            RealMessageBanner::AutoDelete { timestamp } => Self::AutoDelete {
                timestamp: timestamp.into(),
            },
            RealMessageBanner::UnsubscribeNewsletter {
                already_unsubscribed,
            } => Self::UnsubscribeNewsletter {
                already_unsubscribed,
            },
            RealMessageBanner::ScheduledSend { timestamp } => Self::ScheduledSend {
                timestamp: timestamp.into(),
            },
            RealMessageBanner::Snoozed { timestamp } => Self::Snoozed {
                timestamp: timestamp.into(),
            },
            RealMessageBanner::EmbeddedImages => Self::EmbeddedImages,
            RealMessageBanner::RemoteContent => Self::RemoteContent,
            RealMessageBanner::UnableToDecrypt => Self::UnableToDecrypt,
        }
    }
}

#[uniffi_export]
pub async fn message(
    session: Arc<MailUserSession>,
    id: Id,
) -> Result<Option<Message>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        Result::<_, RealProtonMailError>::Ok(
            RealMessage::load(id.into(), &tether).await?.map(Into::into),
        )
    })
    .await
    .map_err(ActionError::from)
}

#[derive(uniffi::Record)]
pub struct WatchedMessage {
    pub message: Message,
    pub handle: Arc<WatchHandle>,
}

declare_live_query_tagger!(WatchMessageMarker);

#[uniffi_export]
pub async fn watch_message(
    session: Arc<MailUserSession>,
    message_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedMessage>, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let Some(message) = RealMessage::open_message(message_id.into(), &user_context).await?
        else {
            return Ok(None);
        };

        let handle = RealMessage::watch(&stash).await?;
        let handle = WatchMessageMarker::watch_channel(&*user_context, handle, callback);
        Result::<_, RealProtonMailError>::Ok(Some(WatchedMessage {
            message: message.into(),
            handle,
        }))
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn scroll_messages_for_label(
    mailbox: Arc<Mailbox>,
    callback: Box<dyn MessageScrollerLiveQueryCallback>,
) -> Result<Arc<MessageScroller>, ActionError> {
    let context = mailbox.ctx()?;

    uniffi_async(async move {
        let label_id = mailbox.label_id();
        let (scroller, handle) =
            MailScroller::messages(context.as_weak(), label_id.into(), 50).await?;

        let handle = spawn_message_scroller_watcher(&context, handle, callback);

        Result::<_, RealProtonMailError>::Ok(Arc::new(MessageScroller::new(scroller, handle)))
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn scroller_search(
    mailbox: Arc<Mailbox>,
    options: PaginatorSearchOptions,
    callback: Box<dyn MessageScrollerLiveQueryCallback>,
) -> Result<Arc<SearchScroller>, ActionError> {
    let context = mailbox.ctx()?;

    uniffi_async(async move {
        let (scroller, handle) =
            MailScroller::search(context.as_weak(), options.into(), 50).await?;

        let handle = spawn_message_scroller_watcher(&context, handle, callback);
        let scroller = SearchScroller::new(scroller, handle);

        Result::<_, RealProtonMailError>::Ok(Arc::new(scroller))
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn available_label_as_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<LabelAsAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let actions = RealMessage::available_label_as_actions(ids.map_vec(), &tether)
            .await?
            .map_vec();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn available_move_to_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<MoveAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let tether = stash.connection().await?;
        let view = RealLabel::load(view, &tether)
            .await?
            .ok_or_else(|| RealProtonMailError::reason(RealActionErrorReason::UnknownLabel))?;
        let actions = RealMessage::available_move_to_actions(
            view,
            ids.into_iter().map_into().collect(),
            &tether,
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Result::<_, RealProtonMailError>::Ok(actions)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn all_available_list_actions_for_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<AllListActions, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let actions = RealMessage::all_available_list_actions_for_messages(
            mailbox.label_id().into(),
            message_ids.map_vec(),
            &tether,
        )
        .await?
        .into();
        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn all_available_message_actions_for_message(
    mailbox: Arc<Mailbox>,
    theme: ThemeOpts,
    message_id: Id,
) -> Result<AllMessageActions, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let actions = RealMessage::all_available_message_actions_for_message(
            current_label_id.into(),
            message_id.into(),
            theme.into(),
            &tether,
        )
        .await?
        .into();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn all_available_message_actions_for_action_sheet(
    mailbox: Arc<Mailbox>,
    theme: ThemeOpts,
    message_id: Id,
) -> Result<MessageActionSheet, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let action_sheet = RealMessage::all_available_message_actions_for_action_sheet(
            current_label_id.into(),
            message_id.into(),
            theme.into(),
            &tether,
        )
        .await?
        .into();

        Ok::<_, RealProtonMailError>(action_sheet)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn get_message_body(
    mbox: &Mailbox,
    id: Id,
) -> Result<Arc<DecryptedMessage>, ActionError> {
    let ctx = mbox.ctx_ptr();
    // We upgrade context to strong reference **temporarily**. The return type of this function is a weak pointer
    // to avoid memory leak
    let strong_ctx = mbox.ctx()?;
    uniffi_async(async move {
        let (sender, body) =
            models::Message::message_body_with_sender(&strong_ctx, id.into()).await?;
        Ok::<_, RealProtonMailError>(Arc::new(DecryptedMessage { ctx, sender, body }))
    })
    .await
    .map_err(ActionError::from)
}

/// Return the boolean value indicating if the message sender is blocked.
///
/// When message is not present in database, it will return `None`.
/// Otherwise, it will return `Some(bool)` where `true` means the sender is blocked
/// and `false` means the sender is not blocked.
#[uniffi_export]
pub async fn is_message_sender_blocked(
    mbox: &Mailbox,
    message_id: Id,
) -> Result<Option<bool>, ActionError> {
    let ctx = mbox.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        Ok::<_, RealProtonMailError>(
            models::Message::is_sender_blocked(message_id.into(), &tether).await?,
        )
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn star_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_star(user_context.action_queue(), message_ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn unstar_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_unstar(user_context.action_queue(), message_ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_read(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    uniffi_async(async move {
        RealMessage::action_mark_read(user_context.action_queue(), message_ids.map_vec())
            .await
            .map_err(RealProtonMailError::from)?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_unread(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    uniffi_async(async move {
        RealMessage::action_mark_unread(user_context.action_queue(), message_ids.map_vec())
            .await
            .map_err(RealProtonMailError::from)?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn delete_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_delete(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_ham(mailbox: Arc<Mailbox>, message_id: Id) -> Result<(), ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        RealMessage::action_ham(ctx.action_queue(), vec![message_id.into()])
            .await
            .map(|()| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn block_address(
    session: Arc<MailUserSession>,
    email: String,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        IncomingDefault::action_block(ctx.action_queue(), email.into())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn unblock_address(mailbox: Arc<Mailbox>, email: String) -> Result<(), ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        IncomingDefault::action_unblock(ctx.action_queue(), email.into())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[allow(unused)]
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn report_phishing(mailbox: Arc<Mailbox>, message_id: Id) -> Result<(), ActionError> {
    let ctx = mailbox.ctx()?;

    uniffi_async(async move {
        RealMessage::action_report_phishing(
            ctx.action_queue(),
            message_id.into(),
            &ctx.user_stash().connection().await?,
        )
        .await
        .map(|()| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[derive(Clone, uniffi::Record)]
pub struct AttachmentData {
    pub data: Vec<u8>,
    pub mime: String,
}

#[uniffi_export]
pub async fn label_messages_as(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
    selected_label_ids: Vec<Id>,
    partially_selected_label_ids: Vec<Id>,
    must_archive: bool,
) -> Result<LabelAsOutput, ActionError> {
    let ctx = mailbox.ctx()?;
    let source_label_id = mailbox.label_id();
    uniffi_async(async move {
        Result::<_, RealProtonMailError>::Ok(
            RealMessage::action_label_as(
                &ctx.user_stash().connection().await?,
                ctx.action_queue(),
                source_label_id.into(),
                message_ids.map_vec(),
                selected_label_ids.map_vec(),
                partially_selected_label_ids.map_vec(),
                must_archive,
            )
            .await?
            .into(),
        )
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn move_messages(
    mailbox: Arc<Mailbox>,
    destination_id: Id,
    message_ids: Vec<Id>,
) -> Result<Option<Arc<Undo>>, ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        RealMessage::action_move(
            &tether,
            ctx.action_queue(),
            destination_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|undo| undo.map(|undo| Arc::new(undo.into())))
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// [`RemoteId`] on its own is useless, because all our UniFFI endpoints operate on
/// local ids. This method translates remote id into local [`Id`].
///
/// It may happen, that the [`RemoteId`] points to the message that does not exist in our
/// database yet. In that case, Rust SDK will fetch necessary information from API before returning the id.
///
#[uniffi_export]
pub async fn resolve_message_id(
    session: Arc<MailUserSession>,
    remote_id: RemoteId,
) -> Result<Id, ActionError> {
    let user_ctx = session.ctx()?;
    uniffi_async(async move {
        let local_id = RealMessage::find_or_fetch_by_remote_id(&user_ctx, remote_id.into()).await?;
        Ok::<_, RealProtonMailError>(local_id.into())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Delete all messages in a label
///
/// Limited to:
///
/// - drafts
/// - spam
/// - trash
/// - custom labels
/// - custom folders
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn delete_all_messages_in_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_delete_all_in_label(
            user_context.action_queue(),
            label_id.into(),
            &user_context.user_stash().connection().await?,
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn update_mobile_list_toolbar_actions(
    session: Arc<MailUserSession>,
    actions: Vec<MobileAction>,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        proton_mail_common::models::MailSettings::action_update_list_toolbar(
            ctx.action_queue(),
            actions.map_vec(),
            false,
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn update_mobile_message_toolbar_actions(
    session: Arc<MailUserSession>,
    actions: Vec<MobileAction>,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        proton_mail_common::models::MailSettings::action_update_message_toolbar(
            ctx.action_queue(),
            actions.map_vec(),
            false,
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[returns(MobileActionsResult)]
pub async fn get_mobile_list_toolbar_actions(
    session: Arc<MailUserSession>,
) -> Result<Vec<MobileAction>, ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        let actions = RealMobileAction::list_toolbar_actions(&tether).await?;
        Result::<_, RealProtonMailError>::Ok(
            actions
                .iter()
                .filter_map(MobileAction::from_real)
                .collect_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[returns(MobileActionsResult)]
pub async fn get_mobile_message_toolbar_actions(
    session: Arc<MailUserSession>,
) -> Result<Vec<MobileAction>, ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        let actions = RealMobileAction::message_toolbar_actions(&tether).await?;
        Result::<_, RealProtonMailError>::Ok(
            actions
                .iter()
                .filter_map(MobileAction::from_real)
                .collect_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
#[must_use]
pub fn get_all_mobile_list_actions() -> Vec<MobileAction> {
    let actions = RealMobileAction::all_list_actions();
    actions
        .iter()
        .filter_map(MobileAction::from_real)
        .collect_vec()
}

#[uniffi_export]
#[must_use]
pub fn get_all_mobile_message_actions() -> Vec<MobileAction> {
    let actions = RealMobileAction::all_message_actions();
    actions
        .iter()
        .filter_map(MobileAction::from_real)
        .collect_vec()
}

/// Bulk check unread status for messages by remote IDs.
///
/// Takes a list of remote message IDs and returns a list of booleans indicating
/// whether each message is unread. The result maintains the same order as the input.
/// For messages that don't exist in the local database, returns true (unread).
///
/// This function is designed to work offline-only for iOS push notification clearing.
#[uniffi_export]
pub async fn bulk_message_unread_status(
    session: Arc<MailUserSession>,
    remote_ids: Vec<RemoteId>,
) -> Result<Vec<bool>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let message_ids: Vec<MessageId> = remote_ids.into_iter().map(Into::into).collect();
        RealMessage::bulk_unread_status_by_remote_ids(message_ids, &tether)
            .await
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
}
