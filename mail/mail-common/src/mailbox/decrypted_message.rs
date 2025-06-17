#![allow(dead_code)]

//! Everything related to processing a decrypted message.
use crate::datatypes::attachment::ContentId;
use crate::datatypes::message_banner::MessageBanner;
use crate::datatypes::theme::MailTheme;
use crate::datatypes::{Disposition, LocalAttachmentId, MimeType};
use crate::models::{
    AttachmentType, EmbeddedAttachmentInfo, MailSettings, Message, MessageBodyMetadata,
};
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use parking_lot::Mutex;
use proton_mail_html_transformer::Transformer;
use proton_mail_html_transformer::transforms::ColorMode;
use proton_mail_html_transformer::transforms::styles::{BrowserCapabilities, IncludeFullStaticCss};
use proton_task_service::AsyncTaskResult;
use serde_json::Value;
use stash::orm::Model;
use stash::stash::Tether;
use std::collections::HashMap;
use std::sync::Arc;
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
    /// Default: No override provided
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
    /// Theme, either provided by the system or overriden by the user
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

/// A message parsed header value can either be a string or an array of strings.
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

type InFlightAttachments =
    HashMap<LocalAttachmentId, JoinHandle<AsyncTaskResult<MailContextResult<Vec<u8>>>>>;

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessageBody {
    /// The decrypted message contents.
    pub body: String,

    /// Metadata associated with the message body
    pub metadata: MessageBodyMetadata,

    /// The subject that comes from a multipart message.
    pub pgp_subject: Option<String>,

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
    /// Create a new instance that immediately starts to pre-download all inline attachments for this
    /// message.
    pub fn new_prefetching(
        body: String,
        metadata: MessageBodyMetadata,
        pgp_subject: Option<String>,
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
                let ctx_clone = ctx.clone();
                let fut = ctx.spawn(async move {
                    let tether = &mut ctx_clone.user_stash().connection();
                    att.content_data(&ctx_clone, tether).await
                });
                (id, fut)
            })
            .collect();

        Self {
            body,
            metadata,
            pgp_subject,
            in_flight: Mutex::new(in_flight),
        }
    }

    /// Create a new instance which does not start to pre-download all attachments for this
    /// message.
    pub fn new_without_prefetching(
        body: String,
        metadata: MessageBodyMetadata,
        pgp_subject: Option<String>,
    ) -> Self {
        Self {
            body,
            metadata,
            pgp_subject,
            in_flight: Default::default(),
        }
    }

    /// Create a new decrypted message body that corresponds to an empty draft with
    /// the given `body` and `mime_type`.
    pub fn new_draft(body: String, mime_type: MimeType) -> Self {
        Self {
            body,
            metadata: MessageBodyMetadata {
                mime_type,
                ..Default::default()
            },
            pgp_subject: None,
            in_flight: Default::default(),
        }
    }

    /// Load or fetch an embedded attachment with `cid` for this message.
    ///
    /// If the attachment is not in the cache it will be downloaded from the server.
    ///
    /// # Errors
    ///
    /// Returns error if the attachments can't be fetched from the server, retrieved
    /// from the cache or the attachment with `cid` does not exist.
    pub async fn get_embedded_attachment(
        &self,
        ctx: &MailUserContext,
        cid: &ContentId,
    ) -> MailContextResult<EmbeddedAttachmentInfo> {
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
                Some(p) => match p.await? {
                    AsyncTaskResult::Completed(Ok(data)) => data,
                    AsyncTaskResult::Cancelled => return Err(MailContextError::TaskCancelled),
                    AsyncTaskResult::Completed(e @ Err(_)) => e?,
                },
                None => {
                    let tether = &mut ctx.user_stash().connection();
                    att.content_data(ctx, tether).await?
                }
            }
        };
        Ok(EmbeddedAttachmentInfo {
            data,
            mime: att.mime_type.to_string(),
            height: att.image_height.clone(),
            width: att.image_width.clone(),
        })
    }

    /// Retrieve a parsed header value for a given `key`.
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        let value = self.metadata.parsed_headers.headers.get(key)?;
        match value {
            Value::String(s) => Some(ParsedHeaderValue::String(s.clone())),
            Value::Array(array) => {
                let mut result = Vec::with_capacity(array.len());
                for (idx, item) in array.iter().enumerate() {
                    if let Value::String(str) = item {
                        result.push(str.clone());
                    } else {
                        tracing::warn!(
                            "Header array value {key}[{idx}] of message {:?} has invalid value type",
                            self.metadata.remote_message_id
                        );
                    }
                }
                Some(ParsedHeaderValue::Array(result))
            }
            _ => {
                tracing::warn!(
                    "Header value {key} of message {:?} has invalid value type",
                    self.metadata.remote_message_id
                );
                None
            }
        }
    }

    /// Gets the message body as an HTML. This does all of the transformations that are
    /// required based on the options and the user settings.
    ///
    /// # Parameters
    ///
    /// * `sender` - the email address of the sender. Example: `test@pm.me`
    ///
    /// # Errors
    ///
    /// Returns an error if the network request, the database query, reading/writing
    /// the body to the cache, or decrypting the body fails,
    /// or if the message doesn't exist.
    pub async fn transformed(
        &self,
        sender: &str,
        opts: TransformOpts,
        tether: &Tether,
    ) -> BodyOutput {
        // FIXME:(perf) settings get loaded twice.
        let resolved = opts.resolve(tether).await;

        let banners = if let Some(id) = self.metadata.local_message_id {
            if let Ok(Some(message)) = Message::load(id, tether).await {
                message.get_banners(tether).await
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        transform_html_with_banners(
            sender,
            // At this point in time we do not have a list of trusted senders.
            // We also do not store that in the database as there is no syncing with the server.
            &[],
            &self.body,
            resolved,
            self.metadata.mime_type,
            banners,
        )
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

/// # Parameters
/// * `sender` - the email address of the sender. Example: `test@pm.me`
/// * `trusted_senders` - list of senders (email addresses, example: `test@pm.me`) that we trust that they support dark mode natively.
pub fn transform_html(
    sender: &str,
    trusted_senders: &[&str],
    html: &str,
    opts: TransformOptsResolved,
    mime_type: MimeType,
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
    mime_type: MimeType,
    mut prev_banners: Vec<MessageBanner>,
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
    let mut transformer = if mime_type == MimeType::TextPlain {
        let mut transformer = Transformer::new_text_plain(html);
        let tok = transformer.add_noreferrer();
        transformer.insert_links(tok);
        transformer
    } else {
        let mut transformer = Transformer::new(html);
        transformer.add_noreferrer();
        transformer
    };
    let tags_stripped = transformer.strip_whitelist();
    let utm_stripped = transformer.strip_utm();

    let (mut remote_images_count, mut embedded_images_count) =
        transformer.disable_content(hide_remote_images, hide_embedded_images);

    let had_blockquote = if !show_block_quote {
        transformer.strip_blockquote()
    } else {
        false
    };

    if cfg!(target_os = "ios") {
        transformer.inject_ios_content_size();
    }

    transformer.inject_dark_mode(
        sender,
        theme.color_mode(),
        BrowserCapabilities {
            supports_dark_mode_via_media_query: theme.supports_dark_mode_via_media_query,
        },
        IncludeFullStaticCss::Yes,
        trusted_senders,
    );

    if opts.hide_remote_images && remote_images_count > 0 {
        prev_banners.push(MessageBanner::RemoteContent);
        // So that they don't show up in the stats later on
        remote_images_count = 0;
    }

    if opts.hide_embedded_images && embedded_images_count > 0 {
        prev_banners.push(MessageBanner::EmbeddedImages);
        // So that they don't show up in the stats later on
        embedded_images_count = 0;
    }

    prev_banners.sort_unstable();

    let output = BodyOutput {
        body: transformer.to_string(),
        had_blockquote,
        tags_stripped,
        utm_stripped,
        remote_images_disabled: remote_images_count,
        embedded_images_disabled: embedded_images_count,
        transform_opts: opts.into(),
        body_banners: prev_banners,
    };
    debug!("HTML Transform done");
    output
}
