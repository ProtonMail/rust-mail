#![allow(dead_code)]

//! Everything related to processing a decrypted message.
use crate::ImagePolicy;
use crate::TrackerService;
use crate::actions::messages::UnsubscribeNewsletter;
use crate::datatypes::attachment::ContentId;
use crate::datatypes::message_banner::MessageBanner;
use crate::datatypes::theme::MailTheme;
use crate::datatypes::{Disposition, LocalAttachmentId, ParsedHeaderValue};
use crate::models::{
    Attachment, AttachmentData, AttachmentType, MailSettings, Message, MessageBodyMetadata,
    MessageMimeType, RawMessageBody,
};
use crate::rsvp::RsvpEventId;
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use anyhow::Context;
use parking_lot::Mutex;
use proton_action_queue::action::ActionId;
use proton_calendar_common::{self as cal, RsvpError};
use proton_core_api::services::proton::AddressId;
use proton_core_common::AddressKeysContactFetchPolicy;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_account::proton_crypto::new_pgp_provider;
use proton_crypto_inbox::lock_icon::{MailVerificationStatus, XPmOrigin};
use proton_crypto_inbox::lock_icon::{UiLock, XPmContentEncryption, XPmRecipientEncryption};
use proton_crypto_inbox::message::DecryptedBody;
use proton_crypto_inbox::proton_crypto::crypto::VerificationError;
use proton_crypto_inbox::proton_crypto_inbox_mime::{ProcessedBodyType, ProcessedMessage};
use proton_mail_html_transformer::Transformer;
use proton_mail_html_transformer::sanitizer::StripStyleSheets;
use proton_mail_html_transformer::transforms::ColorMode;
use proton_mail_html_transformer::transforms::styles::{BrowserCapabilities, IncludeFullStaticCss};
use proton_mail_html_transformer::utm::StrippedUTM;
use stash::orm::Model;
use stash::stash::Tether;
use std::collections::BTreeSet;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs;
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

/// What to do with the body. If in any of the fields `None` is specified it will read the relevant
/// value from the user setttings. If all are set, the db query will be elided.
#[derive(Debug, Clone, Copy)]
pub struct TransformOpts {
    /// Whether should show block quotes or not. Default: true
    pub show_block_quote: bool,
    /// Whether should hide remote images or not. Default: defined in mail settings
    pub hide_remote_images: Option<bool>,
    /// Whether should hide embedded images or not. Default: defined in mail settings
    pub hide_embedded_images: Option<bool>,
    /// Current settings related to the color scheme.
    /// It affects on which CSS style is used in the HTML body of the message
    ///
    /// Default: None
    /// It assumes that the device supports `@media` queries. In that case
    /// passing theme would be irrelevant.
    ///
    pub theme: Option<ThemeOpts>,
}

/// Current settings related to the color scheme.
/// It affects on which CSS style is used in the HTML body of the message
#[derive(Debug, Clone, Copy)]
pub struct ThemeOpts {
    /// What is the current UI color scheme, provided by the application.
    ///
    pub current_theme: MailTheme,
    /// While using the dark mode, some bodies of messages might be hard to read.
    /// User has an option to override the theme inside of the message (without changing the overall theme).
    ///
    /// Default: No override provided.
    ///
    pub theme_override: Option<MailTheme>,

    /// Whether the device supports `@media (prefers-color-scheme: dark) {}` or not.
    ///
    /// Default: True - only Android 9 does not support it (so far)
    ///
    pub supports_dark_mode_via_media_query: bool,
}

impl ThemeOpts {
    pub fn color_mode(&self) -> ColorMode {
        match self.theme() {
            MailTheme::LightMode => ColorMode::LightMode,
            MailTheme::DarkMode => ColorMode::DarkMode,
        }
    }
    /// Theme, either provided by the system or overridden by the user
    pub fn theme(&self) -> MailTheme {
        self.theme_override.unwrap_or(self.current_theme)
    }

    /// Default values assuming that the device is modern enough
    /// to support `@media (prefers-color-scheme: dark)` CSS rule.
    pub fn for_modern_device() -> Self {
        Self {
            supports_dark_mode_via_media_query: true,
            // That value is irrelevant at this point.
            current_theme: MailTheme::DarkMode,
            theme_override: None,
        }
    }
}

impl Default for TransformOpts {
    fn default() -> Self {
        Self {
            show_block_quote: true,
            hide_remote_images: None,
            hide_embedded_images: None,
            theme: None,
        }
    }
}

/// This is created after calling [`TransformOpts::fill_defaults`]
// It exists for type safety purposes.
#[derive(Debug, Clone, Copy)]
pub struct TransformOptsResolved {
    pub show_block_quote: bool,
    pub hide_remote_images: bool,
    pub hide_embedded_images: bool,
    pub theme: ThemeOpts,
}

impl TransformOpts {
    /// Loads the relevant opts from the setttings.
    /// If all are set, the db query will be elided.
    #[must_use]
    pub async fn resolve(self, tether: &Tether) -> TransformOptsResolved {
        let show_block_quote = self.show_block_quote;
        if let (Some(hide_embedded_images), Some(hide_remote_images)) =
            (self.hide_embedded_images, self.hide_remote_images)
        {
            return TransformOptsResolved {
                show_block_quote,
                hide_remote_images,
                hide_embedded_images,
                theme: self.theme.unwrap_or_else(ThemeOpts::for_modern_device),
            };
        }

        let mail_settings = MailSettings::get_or_default(tether).await;
        let MailSettings {
            hide_remote_images,
            hide_embedded_images,
            ..
        } = mail_settings;

        TransformOptsResolved {
            show_block_quote,
            hide_remote_images: self.hide_remote_images.unwrap_or(hide_remote_images),
            hide_embedded_images: self.hide_embedded_images.unwrap_or(hide_embedded_images),
            theme: self.theme.unwrap_or_else(ThemeOpts::for_modern_device),
        }
    }
}

impl From<TransformOptsResolved> for TransformOpts {
    fn from(val: TransformOptsResolved) -> Self {
        TransformOpts {
            show_block_quote: val.show_block_quote,
            hide_remote_images: Some(val.hide_remote_images),
            hide_embedded_images: Some(val.hide_embedded_images),
            theme: Some(val.theme),
        }
    }
}

type InFlightAttachments = HashMap<LocalAttachmentId, JoinHandle<MailContextResult<Vec<u8>>>>;

pub struct DecryptedMessageBody {
    pub body: String,
    pub metadata: MessageBodyMetadata,
    pub mime_type: MessageMimeType,
    pub pgp_subject: Option<String>,
    pub address_id: AddressId,
    pub decryption_error: Option<String>,

    /// Since the clients are holding on to this, we can request the attachments when we are
    /// decrypyting the message so that the data is ready for when they request it.
    ///
    /// Eventually we will want to move to some sort of globally syncrhonized download manager but
    /// for now this will be enough.
    ///
    /// This is necessary because it seems that in iOS the webview is requesting the attachments
    /// one by one.
    pub in_flight: Mutex<InFlightAttachments>,
}

impl DecryptedMessageBody {
    pub fn new_prefetching(
        body: String,
        metadata: MessageBodyMetadata,
        mime_type: MessageMimeType,
        pgp_subject: Option<String>,
        address_id: AddressId,
        decryption_error: Option<String>,
        ctx: Arc<MailUserContext>,
    ) -> Self {
        let in_flight = metadata
            .attachments
            .iter()
            .filter(|att| {
                // These are the only ones we care about since they are the ones
                // that block the msg from displaying quickly.
                att.disposition == Disposition::Inline
                    // We don't fetch atts that already exist
                    && att.attachment_type != AttachmentType::Pgp
            })
            .cloned()
            .map(|att| {
                let id = att.id();

                let fut = ctx.spawn_ex(async move |ctx| {
                    let tether = &mut ctx.user_stash().connection().await?;
                    att.content_data(&ctx, tether).await
                });

                (id, fut)
            })
            .collect();

        Self {
            body,
            metadata,
            mime_type,
            pgp_subject,
            address_id,
            in_flight: Mutex::new(in_flight),
            decryption_error,
        }
    }

    pub fn new_without_prefetching(
        body: String,
        metadata: MessageBodyMetadata,
        mime_type: MessageMimeType,
        pgp_subject: Option<String>,
        address_id: AddressId,
        decryption_error: Option<String>,
    ) -> Self {
        Self {
            body,
            metadata,
            mime_type,
            pgp_subject,
            address_id,
            in_flight: Default::default(),
            decryption_error,
        }
    }

    pub fn from_raw_message_body(
        ctx: Arc<MailUserContext>,
        metadata: MessageBodyMetadata,
        address_id: AddressId,
        raw_body: RawMessageBody,
    ) -> Self {
        match raw_body.into_raw_decrypted_body() {
            Ok(raw_decrypted_body) => match raw_decrypted_body.processed_body() {
                Ok(decrypted_body) => {
                    Self::from_decrypted_body(ctx, metadata, address_id, decrypted_body, false)
                }
                Err(e) => Self::not_decryptable(
                    String::from("Unable to decrypt"),
                    metadata,
                    MessageMimeType::TextPlain,
                    address_id,
                    e.to_string(),
                ),
            },
            Err(error) => Self::not_decryptable(
                error.body,
                metadata,
                MessageMimeType::TextPlain,
                address_id,
                error.error,
            ),
        }
    }

    pub fn from_decrypted_body(
        ctx: Arc<MailUserContext>,
        metadata: MessageBodyMetadata,
        address_id: AddressId,
        decrypted_body: DecryptedBody,
        with_attachment_prefetch: bool,
    ) -> Self {
        let mime_type = MessageMimeType::from_api(metadata.mime_type, || match &decrypted_body {
            DecryptedBody::Plain(_) => unreachable!(),
            DecryptedBody::Mime(msg) => match msg.mime_body_type {
                ProcessedBodyType::Text => MessageMimeType::TextPlain,
                ProcessedBodyType::Html | ProcessedBodyType::Empty => MessageMimeType::TextHtml,
            },
        });

        match decrypted_body {
            DecryptedBody::Plain(body) => {
                if with_attachment_prefetch {
                    DecryptedMessageBody::new_prefetching(
                        body,
                        metadata,
                        mime_type,
                        None,
                        address_id.clone(),
                        None,
                        ctx,
                    )
                } else {
                    DecryptedMessageBody::new_without_prefetching(
                        body,
                        metadata,
                        mime_type,
                        None,
                        address_id.clone(),
                        None,
                    )
                }
            }

            DecryptedBody::Mime(ProcessedMessage {
                body,
                encrypted_subject,
                ..
            }) => {
                if with_attachment_prefetch {
                    DecryptedMessageBody::new_prefetching(
                        body,
                        metadata,
                        mime_type,
                        encrypted_subject,
                        address_id.clone(),
                        None,
                        ctx,
                    )
                } else {
                    DecryptedMessageBody::new_without_prefetching(
                        body,
                        metadata,
                        mime_type,
                        encrypted_subject,
                        address_id.clone(),
                        None,
                    )
                }
            }
        }
    }

    pub fn not_decryptable(
        body: String,
        metadata: MessageBodyMetadata,
        mime_type: MessageMimeType,
        address_id: AddressId,
        error: String,
    ) -> Self {
        Self::new_without_prefetching(body, metadata, mime_type, None, address_id, Some(error))
    }

    pub async fn load_image(
        &self,
        ctx: &MailUserContext,
        url: &str,
        policy: ImagePolicy,
    ) -> MailContextResult<AttachmentData> {
        ctx.image_loader()
            .load(url, policy, async |cid| {
                self.get_embedded_attachment(ctx, cid).await
            })
            .await
            .map_err(Into::into)
    }

    /// Load or fetch an embedded attachment with `cid` for this message.
    ///
    /// If the attachment is not in the cache it will be downloaded from the server.
    pub async fn get_embedded_attachment(
        &self,
        ctx: &MailUserContext,
        cid: &ContentId,
    ) -> MailContextResult<AttachmentData> {
        // We use this for logging if no embedded image was found.
        let mut available_cids = vec![];
        let mut cid_match = |x: &ContentId| {
            if x == cid {
                true
            } else {
                available_cids.push(x.clone());
                false
            }
        };

        let Some(att) = self
            .metadata
            .attachments
            .iter()
            // Notice that we don't check for the disposition, this is intentional.
            .find(|at| at.content_id.as_ref().is_some_and(&mut cid_match))
        else {
            return Err(AppError::UnknownCid(cid.clone(), available_cids).into());
        };

        let data = {
            // We first remove the task from the mutex to avoid locking other threads.
            let task_handle = { self.in_flight.lock().remove(&att.id()) };
            match task_handle {
                Some(p) => match p.await {
                    Ok(Ok(data)) => data,
                    Ok(Err(e)) => Err(e)?,
                    Err(_) => return Err(MailContextError::TaskCancelled),
                },
                None => {
                    let tether = &mut ctx.user_stash().connection().await?;
                    att.content_data(ctx, tether).await?
                }
            }
        };
        Ok(AttachmentData {
            data,
            mime: att.mime_type.to_string(),
        })
    }

    pub fn unsubscribe_from_newsletter(&self) -> anyhow::Result<UnsubscribeNewsletter> {
        UnsubscribeNewsletter::new(
            &self.metadata.parsed_headers,
            self.metadata.local_message_id.unwrap(),
        )
        .context("This action wouldn't do anything")
    }

    pub async fn action_unsubscribe_from_newsletter(
        &self,
        ctx: &MailUserContext,
    ) -> Result<ActionId, anyhow::Error> {
        let headers = &self.metadata.parsed_headers;
        let id = self.metadata.local_message_id.unwrap();
        let queue = ctx.action_queue();

        let action =
            UnsubscribeNewsletter::new(headers, id).context("This action wouldn't do anything")?;

        Ok(queue.queue_action(action).await?.id)
    }

    /// Retrieve a parsed header value for a given `key`.
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        self.metadata.parsed_header_value(key)
    }

    /// Gets the message body as an HTML. This does all of the transformations that are
    /// required based on the options and the user settings.
    ///
    /// # Parameters
    ///
    /// * `sender` - the email address of the sender. Example: `test@pm.me`
    pub async fn transformed(
        &self,
        sender: &str,
        opts: TransformOpts,
        ctx: &MailUserContext,
        tether: &Tether,
    ) -> BodyOutput {
        let resolved = opts.resolve(tether).await;

        let mut banners = if let Some(id) = self.metadata.local_message_id
            && let Ok(Some(message)) = Message::load(id, tether).await
        {
            let can_unsubscribe = self.metadata.parsed_headers.can_unsubscribe();
            message.get_banners_inner(tether, can_unsubscribe).await
        } else {
            vec![]
        };

        if self.failed_to_decrypt() {
            banners.push(MessageBanner::UnableToDecrypt);
        }

        let mut output = transform_html_with_banners(
            sender,
            // At this point in time we do not have a list of trusted senders.
            // We also do not store that in the database as there is no syncing with the server.
            &[],
            &self.body,
            resolved,
            self.mime_type,
            banners,
        );

        if let Some(message_id) = self.metadata.local_message_id {
            let urls_clone = output.remote_urls.clone();
            // We are not using that field outside of this function
            let utm_stripped = std::mem::take(&mut output.utm_stripped);
            ctx.spawn_ex(move |ctx_clone| async move {
                let tracker_service = ctx_clone.get_service::<TrackerService>();
                if let Err(e) = tracker_service
                    .update(message_id, urls_clone, utm_stripped)
                    .await
                {
                    tracing::error!("Could not update tracker information: {e}");
                }
            });
        }

        output
    }

    /// Checks if this mail contains an invitation and, if so, returns its
    /// identifier.
    ///
    /// Use [`RsvpEventId::fetch()`] to fetch the invitation object.
    ///
    /// TODO (NGC-57) implement support for offline-mode
    #[tracing::instrument(skip(self, ctx))]
    pub async fn identify_rsvp(
        &self,
        ctx: &MailUserContext,
    ) -> MailContextResult<Option<RsvpEventId>> {
        let Some(msg_id) = self.metadata.local_message_id else {
            return Ok(None);
        };

        // ---

        let tether = ctx.user_stash().connection().await?;

        let Some(address) = Address::find_by_remote_id(self.address_id.clone(), &tether).await?
        else {
            return Ok(None);
        };

        // Currently we don't support RSVP wiget for BYOE addresses
        if let Some(flags) = address.flags
            && flags.is_byoe()
        {
            return Ok(None);
        }

        drop(tether);

        // ---

        if let Some(id) = cal::RsvpEventId::from_headers(&self.metadata.parsed_headers.headers) {
            debug!("Identified RSVP via headers");

            return Ok(Some(RsvpEventId::new(id, msg_id, self.metadata.clone())));
        }

        let invite = self.metadata.attachments.iter().find_map(|att| {
            if att.mime_type.is_calendar() {
                att.local_id
            } else {
                None
            }
        });

        if let Some(invite) = invite {
            debug!("Analyzing invite attachment");

            let mut tether = ctx.user_stash().connection().await?;

            let ics = Attachment::get_attachment(ctx, invite, &mut tether)
                .await
                .map_err(|err| {
                    warn!(?err, "Couldn't get the RSVP attachment");
                    err
                })?;

            drop(tether);

            let ics = fs::read(&ics.data_path).await.map_err(|err| {
                warn!(?err, "Couldn't read the RSVP attachment");
                err
            })?;

            match cal::RsvpEventId::from_invite(&ics) {
                Ok(id) => {
                    debug!("Identified RSVP via attachment");

                    return Ok(Some(RsvpEventId::new(id, msg_id, self.metadata.clone())));
                }

                Err(RsvpError::IcsIsNotRsvp) => {
                    return Ok(None);
                }

                Err(err) => {
                    warn!(?err, "Couldn't parse the RSVP attachment");

                    return Err(err.into());
                }
            };
        }

        Ok(None)
    }

    pub fn failed_to_decrypt(&self) -> bool {
        self.decryption_error.is_some()
    }

    pub async fn privacy_lock(&self, tether: &Tether) -> PrivacyLockBuilder {
        let Ok(Some(message)) = Message::find_by_id(
            self.metadata.local_message_id.expect("should be set"),
            tether,
        )
        .await
        else {
            tracing::error!("Could not resolve local sent label id");
            return PrivacyLockBuilder::None;
        };

        let origin_header = self.metadata.parsed_header_value(XPmOrigin::header_key());
        let content_encryption_header = self
            .metadata
            .parsed_header_value(XPmContentEncryption::header_key());
        if message.flags.is_draft() || message.flags.is_sent() || message.flags.is_schedule_send() {
            PrivacyLockBuilder::DraftOrSent {
                origin_header,
                content_encryption_header,
                recipient_encryption_header: self
                    .metadata
                    .parsed_header_value(XPmRecipientEncryption::header_key()),
            }
        } else {
            PrivacyLockBuilder::Default {
                origin_header,
                content_encryption_header,
                message: Box::new(message),
            }
        }
    }
}

// Calculating the privacy lock icon can be an expensive process, so we delegate all
// the work to this type tha can be invoked in the background as required.
pub enum PrivacyLockBuilder {
    None,
    DraftOrSent {
        origin_header: Option<ParsedHeaderValue>,
        content_encryption_header: Option<ParsedHeaderValue>,
        recipient_encryption_header: Option<ParsedHeaderValue>,
    },
    Default {
        origin_header: Option<ParsedHeaderValue>,
        content_encryption_header: Option<ParsedHeaderValue>,
        message: Box<Message>,
    },
}

impl PrivacyLockBuilder {
    #[tracing::instrument(skip_all)]
    pub async fn build(self, ctx: &MailUserContext) -> UiLock {
        match self {
            PrivacyLockBuilder::None => UiLock::default(),
            PrivacyLockBuilder::DraftOrSent {
                origin_header,
                content_encryption_header,
                recipient_encryption_header,
            } => Self::build_draft_or_sent(
                origin_header,
                content_encryption_header,
                recipient_encryption_header,
            ),
            PrivacyLockBuilder::Default {
                origin_header,
                content_encryption_header,
                message,
            } => Self::build_default(ctx, origin_header, content_encryption_header, message).await,
        }
    }
    fn build_draft_or_sent(
        origin_header: Option<ParsedHeaderValue>,
        content_encryption_header: Option<ParsedHeaderValue>,
        recipient_encryption_header: Option<ParsedHeaderValue>,
    ) -> UiLock {
        let Some(ParsedHeaderValue::String(origin)) = origin_header else {
            warn!("X-Pm-Origin header missing or not a string");
            return UiLock::default();
        };

        let Some(ParsedHeaderValue::String(content_encryption)) = content_encryption_header else {
            warn!("X-Pm-Content-Encryption header missing or not a string");
            return UiLock::default();
        };

        let Some(ParsedHeaderValue::String(recipient_encryption)) = recipient_encryption_header
        else {
            warn!("X-Pm-Recipient-Encryption header missing or not a string");
            return UiLock::default();
        };

        let Ok(origin) = XPmOrigin::from_str(&origin).inspect_err(|e| {
            warn!(?e, "Could not parse X-Pm-Origin");
        }) else {
            return UiLock::default();
        };

        let Ok(recipient_encryption) = XPmRecipientEncryption::from_header(&recipient_encryption)
            .inspect_err(|e| {
                warn!(?e, "Could not parse X-Pm-Recipient-Encryption");
            })
        else {
            return UiLock::default();
        };

        let Ok(content_encryption) = XPmContentEncryption::from_str(&content_encryption)
            .inspect_err(|e| warn!("X-Pm-Content-Encryption has invalid value: {e}"))
        else {
            return UiLock::default();
        };

        UiLock::for_sent_inbox(origin, content_encryption, &recipient_encryption)
    }

    async fn build_default(
        ctx: &MailUserContext,
        origin_header: Option<ParsedHeaderValue>,
        content_encryption_header: Option<ParsedHeaderValue>,
        message: Box<Message>,
    ) -> UiLock {
        let Some(ParsedHeaderValue::String(origin)) = origin_header else {
            warn!("X-Pm-Origin header missing or not a string");
            return UiLock::default_incoming();
        };

        let Some(ParsedHeaderValue::String(content_encryption)) = content_encryption_header else {
            warn!("X-Pm-Content-Encryption header missing or not a string");
            return UiLock::default_incoming();
        };

        let Ok(origin) = XPmOrigin::from_str(&origin).inspect_err(|e| {
            warn!(?e, "Could not parse X-Pm-Origin");
        }) else {
            return UiLock::default_incoming();
        };

        let Ok(content_encryption) = XPmContentEncryption::from_str(&content_encryption)
            .inspect_err(|e| warn!("X-Pm-Content-Encryption has invalid value: {e}"))
        else {
            return UiLock::default_incoming();
        };

        let Ok(mut tether) = ctx.user_stash().connection().await else {
            warn!("Could not acquire db connection");
            return UiLock::default_incoming();
        };

        let pgp = new_pgp_provider();
        let verification_prefs = match ctx
            .sender_verification_preferences(
                &pgp,
                &mut tether,
                message.sender.address.as_ref(),
                AddressKeysContactFetchPolicy::AllowCachedFallback,
            )
            .await
        {
            Ok(prefs) => {
                debug!("Keys for signature verification: {prefs}");
                prefs
            }
            Err(e) => {
                warn!(?e, "Could not get sender verification preferences");
                return UiLock::default_incoming();
            }
        };

        if !verification_prefs.uses_pinned_keys() || verification_prefs.self_owned_keys() {
            return UiLock::for_receive_inbox(
                origin,
                content_encryption,
                MailVerificationStatus::NotVerified,
                Some(&verification_prefs),
            );
        }

        let Ok(Some(raw_message)) = RawMessageBody::load(message.id(), &tether).await else {
            warn!("Could not find {:?} body", message.id());
            return UiLock::default_incoming();
        };

        let Ok(raw_decrypted_message) = raw_message.into_raw_decrypted_body() else {
            warn!("{:?} does not have a valid message body", message.id());
            return UiLock::default_incoming();
        };

        let Ok(lock) = tokio::task::spawn_blocking(move || {
            let verification_result = raw_decrypted_message
                .verify_signature(&pgp, verification_prefs.signature_verification_keys())
                .inspect_err(|e| {
                    match e {
                        // This happens very frequently, should not be logged.
                        VerificationError::NotSigned(_) => {}
                        e => {
                            tracing::error!(
                                "Failed to verify signature for {:?}: {e}",
                                message.id()
                            );
                        }
                    }
                });
            UiLock::for_receive_inbox(
                origin,
                content_encryption,
                verification_result.into(),
                Some(&verification_prefs),
            )
        })
        .await
        else {
            warn!("failed to join blocking task");
            return UiLock::default_incoming();
        };

        lock
    }
}

/// The result of transforming the message body.
/// It will have more things in the future
#[non_exhaustive]
#[derive(Clone, derive_more::derive::Debug)]
pub struct BodyOutput {
    /// The transformed html of the message.
    #[debug("{} bytes", body.len())]
    pub body: String,

    /// Whether or not [`RemoteContent::Strip`] removed a blockquote.
    pub had_blockquote: bool,

    /// How many html tags it has removed.
    pub tags_stripped: u64,

    /// Set of UTM tracking params that were removed.
    pub utm_stripped: BTreeSet<StrippedUTM>,

    /// Set of remote URLs that were found
    pub remote_urls: HashSet<String>,

    /// The transform opts that were used. All fields are actually Some.
    pub transform_opts: TransformOpts,

    /// This instructs the client on what banners they should show.
    pub body_banners: Vec<MessageBanner>,
}

/// # Parameters
/// * `sender` - the email address of the sender. Example: `test@pm.me`
/// * `trusted_senders` - list of senders (email addresses, example: `test@pm.me`) that we trust that they support dark mode natively.
pub fn transform_html(
    sender: &str,
    trusted_senders: &[&str],
    html: &str,
    opts: TransformOptsResolved,
    mime_type: MessageMimeType,
) -> BodyOutput {
    transform_html_with_banners(sender, trusted_senders, html, opts, mime_type, vec![])
}

/// # Parameters
/// * `sender` - the email address of the sender. Example: `test@pm.me`
/// * `trusted_senders` - list of senders (email addresses, example: `test@pm.me`) that we trust that they support dark mode natively.
#[tracing::instrument(skip_all)]
pub fn transform_html_with_banners(
    sender: &str,
    trusted_senders: &[&str],
    html: &str,
    opts: TransformOptsResolved,
    mime_type: MessageMimeType,
    mut banners: Vec<MessageBanner>,
) -> BodyOutput {
    trace!(
        "\
Beginning html transform:
opts: {opts:#?}
mime_type: {mime_type:?}"
    );

    // The order at which we run the transforms is not random, it's been chosen for maximum
    // efficiency.
    let TransformOptsResolved {
        show_block_quote,
        hide_remote_images,
        hide_embedded_images,
        theme,
    } = opts;

    // If the message is text/plain we need to apply some extra transforms to it like
    // preserving whitespaces and adding links.
    let mut transformer = if mime_type == MessageMimeType::TextPlain {
        let mut transformer = Transformer::new_text_plain(html);
        let tok = transformer.add_noreferrer();
        transformer.insert_links(tok);
        transformer
    } else {
        let mut transformer = Transformer::new(html);
        transformer.add_noreferrer();
        transformer
    };

    let tags_stripped = transformer.strip_whitelist(StripStyleSheets::No);
    let utm_stripped = transformer.strip_utm();

    let disable_output = transformer.disable_content(hide_remote_images, hide_embedded_images);
    let remote_urls = disable_output.remote_urls;

    let had_blockquote = if !show_block_quote {
        transformer.strip_blockquote()
    } else {
        false
    };

    if cfg!(target_os = "ios") {
        transformer.inject_ios_content_size();
    }

    transformer.transform_to_proton_schemes();

    transformer.inject_dark_mode(
        sender,
        theme.color_mode(),
        BrowserCapabilities {
            supports_dark_mode_via_media_query: theme.supports_dark_mode_via_media_query,
        },
        IncludeFullStaticCss::Yes,
        trusted_senders,
    );

    transformer.inject_common_css();

    if opts.hide_remote_images && !remote_urls.is_empty() {
        banners.push(MessageBanner::RemoteContent);
    }

    if opts.hide_embedded_images && !disable_output.embedded_urls.is_empty() {
        banners.push(MessageBanner::EmbeddedImages);
    }

    banners.sort_unstable();

    let output = BodyOutput {
        body: transformer.to_string(),
        had_blockquote,
        tags_stripped,
        utm_stripped,
        remote_urls,
        transform_opts: opts.into(),
        body_banners: banners,
    };

    trace!("HTML Transform done");

    output
}
