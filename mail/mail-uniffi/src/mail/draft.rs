mod attachments;
mod observer;
mod recipients;

use super::ImagePolicy;
use crate::core::datatypes::{Id, UnixTimestamp};
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{
    AttachmentDataResult, DraftCancelScheduleSendError, DraftDiscardError, DraftExpirationError,
    DraftOpenError, DraftPasswordError, DraftSaveError, DraftSendError,
    DraftSenderAddressChangeError, DraftUndoSendError, ProtonError, VoidDraftDiscardResult,
    VoidDraftExpirationResult, VoidDraftPasswordResult, VoidDraftSaveResult, VoidDraftSendResult,
    VoidDraftUndoSendResult,
};
use crate::mail::MailUserSession;
use crate::mail::datatypes::MimeType;
use crate::mail::draft::attachments::AttachmentList;
use crate::mail::draft::observer::DraftSendResult;
use crate::mail::draft::recipients::{ComposerRecipient, ComposerRecipientValidationCallback};
use crate::mail::messages::{AttachmentData, ThemeOpts};
use crate::mail::state::MailUserContextPtr;
use crate::{async_runtime, uniffi_async};
use chrono::Local;
use proton_core_api::services::proton::PrivateEmail;
use proton_mail_common::draft::recipients::ExpirationFeatureSupportReport;
use proton_mail_common::draft::{
    Draft as RealDraft, DraftActorOptions, DraftEvent,
    DraftExpirationTime as RealDraftExpirationTime, DraftSyncStatus as RealDraftSyncStatus, EoData,
    RecipientGroupId, ReplyMode, ScheduleSendOptions,
    compose::DraftAddressValidationError as RealDraftAddressValidationError,
    compose::DraftAddressValidationResult as RealDraftAddressValidationResult,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::DraftSendResult as RealDraftSendResult;
use proton_mail_common::models::{DraftMetadata, MessageMimeType};
use proton_mail_common::{MailContextError, MailUserContext};
use recipients::ComposerRecipientList;
use secrecy::{ExposeSecret, SecretString};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};

#[derive(Debug, Copy, Clone, uniffi::Enum)]
pub enum DraftCreateMode {
    Empty,
    Reply(Id),
    ReplyAll(Id),
    Forward(Id),
    FromIosShareExtension,
}

#[derive(Debug, uniffi::Record)]
pub struct DraftScheduleSendOptions {
    pub tomorrow_time: UnixTimestamp,
    pub monday_time: UnixTimestamp,
    pub is_custom_option_available: bool,
}

#[derive(Debug, uniffi::Record)]
pub struct HtmlForComposer {
    /// HTML content that should be injected into `<head>` tag.
    ///
    /// It does not provide `<head>` tag on its own.
    /// Therefore, the returned HTML can be inserted alongside with other html nodes.
    pub head_content: String,
    /// Initial body of the draft. Usually contains signature
    /// and replied quote.
    pub initial_body: String,
}
impl From<ScheduleSendOptions<Local>> for DraftScheduleSendOptions {
    fn from(value: ScheduleSendOptions<Local>) -> Self {
        Self {
            tomorrow_time: proton_core_common::datatypes::UnixTimestamp::from(value.time_tomorrow)
                .into(),
            monday_time: proton_core_common::datatypes::UnixTimestamp::from(value.time_next_monday)
                .into(),
            is_custom_option_available: value.is_custom_datetime_available,
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum DraftAddressValidationError {
    SubscriptionRequired,
    Disabled,
    CanNotSend,
    CanNotReceive,
}

impl From<RealDraftAddressValidationError> for DraftAddressValidationError {
    fn from(value: RealDraftAddressValidationError) -> Self {
        match value {
            RealDraftAddressValidationError::SubscriptionRequired => Self::SubscriptionRequired,
            RealDraftAddressValidationError::Disabled => Self::Disabled,
            RealDraftAddressValidationError::CanNotSend => Self::CanNotSend,
            RealDraftAddressValidationError::CanNotReceive => Self::CanNotReceive,
        }
    }
}
#[derive(Debug, uniffi::Record)]
pub struct DraftAddressValidationResult {
    pub email: String,
    pub error: DraftAddressValidationError,
}

impl From<RealDraftAddressValidationResult> for DraftAddressValidationResult {
    fn from(value: RealDraftAddressValidationResult) -> Self {
        Self {
            email: value.email,
            error: value.error.into(),
        }
    }
}

#[derive(Debug, uniffi::Record)]
pub struct DraftPassword {
    pub password: String,
    pub hint: Option<String>,
}

impl From<EoData> for DraftPassword {
    fn from(value: EoData) -> Self {
        Self {
            password: value.password.expose_secret().clone(),
            hint: value.password_hint,
        }
    }
}

struct CachedDraftData {
    subject: String,
    body: String,
    mime_type: MessageMimeType,
    send_result: Option<RealDraftSendResult>,
    to_list: Vec<ComposerRecipient>,
    to_list_cb: Option<Arc<dyn ComposerRecipientValidationCallback>>,
    cc_list: Vec<ComposerRecipient>,
    cc_list_cb: Option<Arc<dyn ComposerRecipientValidationCallback>>,
    bcc_list: Vec<ComposerRecipient>,
    bcc_list_cb: Option<Arc<dyn ComposerRecipientValidationCallback>>,
}

/// Represents a draft message which can be crafted as empty or as a reply/forward
/// to an existing message.
#[derive(uniffi::Object)]
pub struct Draft {
    instance: RealDraft,
    cached: Arc<RwLock<CachedDraftData>>,
    ctx: MailUserContextPtr,
    attachment_list: Arc<AttachmentList>,
}
impl Draft {
    async fn new_impl(
        ctx: MailUserContextPtr,
        real_ctx: &MailUserContext,
        draft: proton_mail_common::draft::Draft,
    ) -> Result<Arc<Self>, MailContextError> {
        let state = draft.state().await?;
        let staging_path = draft.attachment_staging_path(real_ctx);

        let cached = Arc::new(RwLock::new(CachedDraftData {
            subject: state.subject,
            body: state.body,
            mime_type: state.mime_type,
            send_result: state.send_result,
            to_list: state
                .to_list
                .recipients()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            cc_list: state
                .cc_list
                .recipients()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            bcc_list: state
                .bcc_list
                .recipients()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            to_list_cb: None,
            cc_list_cb: None,
            bcc_list_cb: None,
        }));

        let cached_cloned = Arc::downgrade(&cached);
        let event_received = draft.subscribe();

        real_ctx.spawn(async move {
            Self::handle_draft_event(cached_cloned, event_received).await;
        });

        Ok(Arc::new_cyclic(|_| Self {
            ctx: ctx.clone(),
            cached,
            attachment_list: AttachmentList::new(ctx, &staging_path, draft.clone()),
            instance: draft,
        }))
    }

    async fn handle_draft_event(
        state: Weak<RwLock<CachedDraftData>>,
        mut receiver: broadcast::Receiver<DraftEvent>,
    ) {
        loop {
            let msg = match receiver.recv().await {
                Ok(msg) => msg,
                Err(broadcast::error::RecvError::Lagged(x)) => {
                    tracing::warn!("Draft Event Observer was {x} events behind");
                    continue;
                }
                Err(_) => {
                    // Draft instance is dead
                    return;
                }
            };

            let Some(state) = state.upgrade() else {
                return;
            };

            match msg {
                DraftEvent::RecipientListUpdated { group, list } => {
                    let mut state = state.write().await;
                    let cb = match group {
                        RecipientGroupId::To => {
                            state.to_list =
                                list.into_recipients().into_iter().map(Into::into).collect();
                            state.to_list_cb.clone()
                        }
                        RecipientGroupId::Cc => {
                            state.cc_list =
                                list.into_recipients().into_iter().map(Into::into).collect();
                            state.cc_list_cb.clone()
                        }
                        RecipientGroupId::Bcc => {
                            state.bcc_list =
                                list.into_recipients().into_iter().map(Into::into).collect();
                            state.bcc_list_cb.clone()
                        }
                    };
                    drop(state);
                    if let Some(cb) = cb {
                        async_runtime().spawn_blocking(move || cb.on_update());
                    }
                }
                DraftEvent::RecipientListsUpdated { to, cc, bcc } => {
                    let mut state = state.write().await;
                    state.to_list = to.into_recipients().into_iter().map(Into::into).collect();
                    let to_cb = state.to_list_cb.clone();
                    state.cc_list = cc.into_recipients().into_iter().map(Into::into).collect();
                    let cc_cb = state.cc_list_cb.clone();
                    state.bcc_list = bcc.into_recipients().into_iter().map(Into::into).collect();
                    let bcc_cb = state.bcc_list_cb.clone();
                    drop(state);

                    if let Some(cb) = to_cb {
                        async_runtime().spawn_blocking(move || cb.on_update());
                    }
                    if let Some(cb) = cc_cb {
                        async_runtime().spawn_blocking(move || cb.on_update());
                    }
                    if let Some(cb) = bcc_cb {
                        async_runtime().spawn_blocking(move || cb.on_update());
                    }
                }
                DraftEvent::Sent | DraftEvent::Discarded => {
                    // disconnect callbacks after message sent
                    let mut state = state.write().await;
                    state.bcc_list_cb = None;
                    state.cc_list_cb = None;
                    state.to_list_cb = None;
                    drop(state);
                }
            }
        }
    }
}

/// Represent an open draft.
#[derive(uniffi::Record)]
pub struct OpenDraft {
    /// The draft object itself.
    pub draft: Arc<Draft>,
    /// Whether the draft was synced from the server or we are operating on a cached version.
    pub sync_status: DraftSyncStatus,
}

/// Indicates whether the draft was synced from the server or we are operating on a cached version.
#[derive(uniffi::Enum)]
pub enum DraftSyncStatus {
    /// Draft was not synced from server and we are operating on a cached version
    Cached,
    /// Draft was synced from server.
    Synced,
}

impl From<RealDraftSyncStatus> for DraftSyncStatus {
    fn from(value: RealDraftSyncStatus) -> Self {
        match value {
            RealDraftSyncStatus::Synced => Self::Synced,
            RealDraftSyncStatus::Cached => Self::Cached,
        }
    }
}

#[derive(uniffi::Record)]
pub struct DraftSenderAddressList {
    /// All available addresses which can be used for sending, also includes the
    /// `active` address.
    pub available: Vec<String>,
    /// The current active address.
    pub active: String,
}

/// Create a new draft with the given `create_mode`.
///
/// # Errors
///
/// Return error if action failed.
///
#[uniffi_export]
pub async fn new_draft(
    session: &MailUserSession,
    create_mode: DraftCreateMode,
    image_policy: ImagePolicy,
) -> Result<Arc<Draft>, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();

    uniffi_async(async move {
        let options = draft_options();

        let draft = match create_mode {
            DraftCreateMode::Empty => RealDraft::empty_ex(&ctx, options).await,

            DraftCreateMode::Reply(id) => {
                RealDraft::reply_ex(
                    &ctx,
                    id.into(),
                    ReplyMode::Sender,
                    image_policy.into(),
                    false,
                    options,
                )
                .await
            }

            DraftCreateMode::ReplyAll(id) => {
                RealDraft::reply_ex(
                    &ctx,
                    id.into(),
                    ReplyMode::All,
                    image_policy.into(),
                    false,
                    options,
                )
                .await
            }

            DraftCreateMode::Forward(id) => {
                RealDraft::reply_ex(
                    &ctx,
                    id.into(),
                    ReplyMode::Forward,
                    image_policy.into(),
                    false,
                    options,
                )
                .await
            }

            DraftCreateMode::FromIosShareExtension => {
                RealDraft::from_ios_share_extension(&ctx, options).await
            }
        }
        .map_err(RealProtonMailError::from)?;

        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ptr, &ctx, draft).await?)
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

/// Open an existing draft with `message_id`.
///
/// # Errors
///
/// Returns error if the query failed or the message is not a draft.
///
#[uniffi_export]
pub async fn open_draft(
    session: &MailUserSession,
    message_id: Id,
) -> Result<OpenDraft, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();
    uniffi_async(async move {
        let options = draft_options();
        let (draft, status) = RealDraft::open_ex(&ctx, message_id.into(), options).await?;
        let draft = Draft::new_impl(ptr, &ctx, draft).await?;
        Ok::<_, RealProtonMailError>(OpenDraft {
            draft,
            sync_status: status.into(),
        })
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

#[uniffi_export]
impl Draft {
    pub fn sender(&self) -> String {
        //TODO: Improve in follow up with event updates.
        async_runtime().block_on(async { self.instance.sender().await.unwrap_or_default() })
    }

    pub fn to_recipients(&self) -> Arc<ComposerRecipientList> {
        ComposerRecipientList::new_to_list(self.instance.clone(), self.cached.clone())
    }

    pub fn cc_recipients(&self) -> Arc<ComposerRecipientList> {
        ComposerRecipientList::new_cc_list(self.instance.clone(), self.cached.clone())
    }

    pub fn bcc_recipients(&self) -> Arc<ComposerRecipientList> {
        ComposerRecipientList::new_bcc_list(self.instance.clone(), self.cached.clone())
    }

    pub fn subject(&self) -> String {
        async_runtime().block_on(async { self.cached.read().await.subject.clone() })
    }

    /// Returns both HTML content for <head> in the composer editor, as well as modified (as in with dark mode)
    /// body of the draft.
    /// Used to initialize the composer editor.
    ///
    /// **WARNING**: This function modifies the draft content by removing `!important` flag.
    ///
    /// # Parameters
    ///
    /// * `theme_opts` - theme options - used to determine html content theme.
    /// * `editor_id` - the HTML ID of the editor that wraps the message. The same used to reference DOM in javascript.
    ///
    /// # Example of usage
    ///
    /// ```ignore
    /// let html_for_composer = draft.html_for_composer(theme_opts, "editor");
    /// let head_to_inject = html_for_composer.head_content;
    /// let initial_body = html_for_composer.initial_body;
    ///
    /// let template = format!("
    /// <html>
    /// <head>
    ///
    ///    <meta ...things set up for the composer />
    ///
    ///    {head_to_inject}
    ///
    /// </head>
    /// <body>
    /// ...
    /// {initial_body}
    /// ...
    /// </body>
    /// </html>
    /// ");
    /// ```
    pub fn html_for_composer(
        &self,
        theme_opts: ThemeOpts,
        editor_id: String,
    ) -> Result<HtmlForComposer, ProtonError> {
        let theme_opts = theme_opts.into();

        Ok(async_runtime().block_on(async {
            let (head_content, initial_body) = self
                .instance
                .html_head_content_for_composer(theme_opts, editor_id)
                .await?;

            Ok::<_, RealProtonMailError>(HtmlForComposer {
                head_content,
                initial_body,
            })
        })?)
    }

    pub fn body(&self) -> String {
        async_runtime().block_on(async { self.cached.read().await.body.clone() })
    }

    #[returns(VoidDraftSaveResult)]
    pub fn set_subject(&self, subject: String) -> Result<(), DraftSaveError> {
        async_runtime()
            .block_on(async {
                let mut instance = self.cached.write().await;
                self.instance.set_subject(subject.clone()).await?;
                instance.subject = subject;
                Ok::<_, RealProtonMailError>(())
            })
            .map_err(DraftSaveError::from)
            .into()
    }

    #[returns(VoidDraftSaveResult)]
    pub fn set_body(&self, body: String) -> Result<(), DraftSaveError> {
        async_runtime()
            .block_on(async {
                let mut instance = self.cached.write().await;
                self.instance.set_body(body.clone()).await?;
                instance.body = body;
                Ok::<_, RealProtonMailError>(())
            })
            .map_err(DraftSaveError::from)
            .into()
    }

    pub fn mime_type(&self) -> MimeType {
        async_runtime().block_on(async { self.cached.read().await.mime_type.into() })
    }

    pub async fn message_id(self: Arc<Self>) -> Result<Option<Id>, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };

        uniffi_async::<Option<Id>, RealProtonMailError, _>(async move {
            let metadata_id = self.instance.metadata_id;
            let tether = ctx.user_stash().connection().await?;

            DraftMetadata::message_id(metadata_id, &tether)
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    /// Retrieve the send result associated with draft.
    ///
    /// Note this only loaded with [`open_draft()`].
    pub fn send_result(&self) -> Option<DraftSendResult> {
        async_runtime()
            .block_on(async { self.cached.read().await.send_result.clone().map(Into::into) })
    }

    // NOTE: iOS request we share the same result types between
    //       this function and the DecryptedMessageBody equivalent.
    #[returns(AttachmentDataResult)]
    pub async fn load_image(self: Arc<Self>, url: String) -> Result<AttachmentData, ProtonError> {
        uniffi_async(async move { self.load_image_impl(url).await })
            .await
            .map_err(ProtonError::from)
            .into()
    }

    // NOTE: iOS request we share the same result types between
    //       this function and the DecryptedMessageBody equivalent.
    #[returns(AttachmentDataResult)]
    pub fn load_image_sync(self: Arc<Self>, cid: String) -> Result<AttachmentData, ProtonError> {
        async_runtime()
            .block_on(self.load_image_impl(cid))
            .map_err(ProtonError::from)
            .into()
    }

    pub fn attachment_list(&self) -> Arc<AttachmentList> {
        Arc::clone(&self.attachment_list)
    }

    pub async fn change_sender_address(
        self: Arc<Self>,
        email: String,
    ) -> Result<(), DraftSenderAddressChangeError> {
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut cached = self.cached.write().await;
            let new_body = self.instance.change_sender_address(email).await?;
            cached.body = new_body;
            Ok(())
        })
        .await
        .map_err(DraftSenderAddressChangeError::from)
    }

    pub async fn list_sender_addresses(
        self: Arc<Self>,
    ) -> Result<DraftSenderAddressList, ProtonError> {
        Ok(uniffi_async::<_, RealProtonMailError, _>(async move {
            let available = self
                .instance
                .sender_addresses()
                .await?
                .into_iter()
                .map(|v| v.email)
                .collect::<Vec<_>>();

            let active = self.instance.sender().await?;

            Ok(DraftSenderAddressList { available, active })
        })
        .await?)
    }
}

#[uniffi_export]
impl Draft {
    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSaveResult)]
    pub async fn save(self: Arc<Self>) -> Result<(), DraftSaveError> {
        uniffi_async(async move {
            self.instance
                .save()
                .await
                .map_err(RealProtonMailError::from)?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSaveError::from)
        .into()
    }

    /// Sends the draft.
    ///
    /// Schedules an action which saves and then sends the draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSendResult)]
    pub async fn send(self: Arc<Self>) -> Result<(), DraftSendError> {
        uniffi_async(async move {
            self.instance
                .send()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSendError::from)
        .into()
    }

    /// Schedule the sending of the given draft at the `timestamp`.
    #[returns(VoidDraftSendResult)]
    pub async fn schedule(self: Arc<Self>, timestamp: UnixTimestamp) -> Result<(), DraftSendError> {
        let timestamp = proton_core_common::datatypes::UnixTimestamp::from(timestamp)
            .to_date_time()
            .ok_or(DraftSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )))?;
        uniffi_async(async move {
            self.instance
                .schedule_send(timestamp)
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSendError::from)
        .into()
    }

    // Mobile requested this to be sync.
    pub fn schedule_send_options(&self) -> Result<DraftScheduleSendOptions, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        async_runtime()
            .block_on(async {
                RealDraft::schedule_send_options(&ctx)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(ProtonError::from)
            .map(Into::into)
    }

    /// Discard the draft.
    ///
    /// Schedules an action which deletes a draft locally and on the server.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftDiscardResult)]
    pub async fn discard(self: Arc<Self>) -> Result<(), DraftDiscardError> {
        uniffi_async(async move {
            self.instance
                .discard()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftDiscardError::from)
        .into()
    }

    pub fn is_password_protected(&self) -> Result<bool, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .is_password_protected()
                .await
                .map_err(RealProtonMailError::from)
        })?)
    }

    pub fn get_password(&self) -> Result<Option<DraftPassword>, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .get_password()
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })?)
    }

    #[returns(VoidDraftPasswordResult)]
    pub async fn set_password(
        self: Arc<Self>,
        password: String,
        hint: Option<String>,
    ) -> Result<(), DraftPasswordError> {
        let password = SecretString::new(password);
        uniffi_async(async move {
            self.instance
                .set_password_with_secret(password, hint)
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftPasswordError::from)
        .into()
    }

    #[returns(VoidDraftPasswordResult)]
    pub async fn remove_password(self: Arc<Self>) -> Result<(), DraftPasswordError> {
        uniffi_async(async move {
            self.instance
                .remove_password()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftPasswordError::from)
        .into()
    }

    #[returns(VoidDraftExpirationResult)]
    pub async fn set_expiration_time(
        self: Arc<Self>,
        expiration_time: DraftExpirationTime,
    ) -> Result<(), DraftExpirationError> {
        let expiration_time = RealDraftExpirationTime::try_from(expiration_time)?;
        uniffi_async(async move {
            self.instance
                .set_expiration_time(expiration_time.into())
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftExpirationError::from)
        .into()
    }

    pub fn expiration_time(&self) -> Result<DraftExpirationTime, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .expiration_time()
                .await
                .map(Into::into)
                .map_err(RealProtonMailError::from)
        })?)
    }

    pub fn address_validation_result(&self) -> Option<DraftAddressValidationResult> {
        async_runtime().block_on(async move {
            self.instance
                .address_validation_result()
                .await
                .unwrap_or(None)
                .map(Into::into)
        })
    }

    pub fn clear_address_validation_error(&self) {
        async_runtime().block_on(async move {
            if let Err(e) = self.instance.clear_address_validation_result().await {
                tracing::error!("failed to clear address validation error: {:?}", e);
            }
        });
    }

    pub fn validate_recipients_expiration_feature(
        &self,
    ) -> Result<DraftRecipientExpirationFeatureReport, ProtonError> {
        let Some(_) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        async_runtime().block_on(async move {
            let report = self
                .instance
                .validate_expiration_feature()
                .await
                .map_err(RealProtonMailError::from)?;
            Ok(report.into())
        })
    }
}

impl Draft {
    async fn load_image_impl(&self, url: String) -> Result<AttachmentData, RealProtonMailError> {
        let att = self
            .instance
            .load_image(url)
            .await
            .map_err(RealProtonMailError::from)?;

        Ok(AttachmentData {
            data: att.data,
            mime: att.mime,
        })
    }
}

/// Cancel the sending of message with `message_id`.
///
/// Note that will only work if the message has been sent with a send delay.
#[uniffi_export]
#[returns(VoidDraftUndoSendResult)]
pub async fn draft_undo_send(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftUndoSendError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        RealDraft::action_undo_send(ctx.action_queue(), message_id.into()).await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftUndoSendError::from)
    .into()
}

/// Discard a Draft by with the given `message_id`.
///
/// Note that this requires that the given message interacted with any of the [`Draft`] APIs
/// in the past.
#[uniffi_export]
#[returns(VoidDraftDiscardResult)]
pub async fn draft_discard(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftDiscardError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        RealDraft::action_discard(message_id.into(), &tether, ctx.action_queue(), ctx.origin())
            .await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftDiscardError::from)
    .into()
}

#[derive(uniffi::Record)]
pub struct DraftCancelScheduledSendInfo {
    pub last_scheduled_time: UnixTimestamp,
}

/// Cancel the scheduled send of message with `message_id`.
///
/// Note that will only work if the message has been scheduled for sending.
#[uniffi_export]
pub async fn draft_cancel_schedule_send(
    session: &MailUserSession,
    message_id: Id,
) -> Result<DraftCancelScheduledSendInfo, DraftCancelScheduleSendError> {
    let ctx = session.ctx()?;
    let old_time = uniffi_async(async move {
        let old_time = RealDraft::cancel_schedule_send(&ctx, message_id.into()).await?;
        Ok::<_, RealProtonMailError>(old_time)
    })
    .await
    .map_err(DraftCancelScheduleSendError::from)?;

    Ok(DraftCancelScheduledSendInfo {
        last_scheduled_time: proton_core_common::datatypes::UnixTimestamp::from(old_time).into(),
    })
}

#[derive(Debug, uniffi::Enum)]
pub enum DraftExpirationTime {
    Never,
    OneHour,
    OneDay,
    ThreeDays,
    Custom(UnixTimestamp),
}

impl From<RealDraftExpirationTime> for DraftExpirationTime {
    fn from(value: RealDraftExpirationTime) -> Self {
        match value {
            RealDraftExpirationTime::Never => DraftExpirationTime::Never,
            RealDraftExpirationTime::OneHour => DraftExpirationTime::OneHour,
            RealDraftExpirationTime::OneDay => DraftExpirationTime::OneDay,
            RealDraftExpirationTime::ThreeDays => DraftExpirationTime::ThreeDays,
            RealDraftExpirationTime::Custom(dt) => DraftExpirationTime::Custom(dt.into()),
        }
    }
}

impl TryFrom<DraftExpirationTime> for RealDraftExpirationTime {
    type Error = DraftExpirationError;

    fn try_from(value: DraftExpirationTime) -> Result<Self, Self::Error> {
        match value {
            DraftExpirationTime::Never => Ok(RealDraftExpirationTime::Never),
            DraftExpirationTime::OneHour => Ok(RealDraftExpirationTime::OneHour),
            DraftExpirationTime::OneDay => Ok(RealDraftExpirationTime::OneDay),
            DraftExpirationTime::ThreeDays => Ok(RealDraftExpirationTime::ThreeDays),
            DraftExpirationTime::Custom(timestamp) => {
                let expiration_time = proton_core_common::datatypes::UnixTimestamp::from(timestamp)
                    .to_date_time()
                    .ok_or(DraftExpirationError::Other(ProtonError::Unexpected(
                        UnexpectedError::Internal,
                    )))?;
                Ok(RealDraftExpirationTime::Custom(expiration_time))
            }
        }
    }
}

#[derive(Default, Debug, uniffi::Record)]
pub struct DraftRecipientExpirationFeatureReport {
    pub supported: Vec<String>,
    pub unsupported: Vec<String>,
    pub unknown: Vec<String>,
}

impl From<ExpirationFeatureSupportReport> for DraftRecipientExpirationFeatureReport {
    fn from(value: ExpirationFeatureSupportReport) -> Self {
        Self {
            supported: value
                .supported
                .into_iter()
                .map(PrivateEmail::into_clear_text_string)
                .collect(),
            unsupported: value
                .unsupported
                .into_iter()
                .map(PrivateEmail::into_clear_text_string)
                .collect(),
            unknown: value
                .unknown
                .into_iter()
                .map(PrivateEmail::into_clear_text_string)
                .collect(),
        }
    }
}

fn draft_options() -> DraftActorOptions {
    DraftActorOptions {
        address_validation_enabled: true,
        // Auto save is set to 0 to mimic old behavior where setting the body would trigger an immediate save.
        // to be reverted after release.
        auto_save_every: Some(Duration::from_secs(0)),
    }
}
