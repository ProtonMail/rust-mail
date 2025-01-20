#![allow(dead_code)]

//! Everything related to processing a decrypted message.

use crate::datatypes::{LocalAttachmentId, MimeType};
use crate::models::{Attachment, EmbeddedAttachmentInfo, MailSettings, MessageBodyMetadata};
use crate::{AppError, MailUserContext};
use parking_lot::Mutex;
use proton_api_core::services::proton::common::AuthId;
use proton_crypto_inbox::proton_crypto_inbox_mime::ProcessedAttachment;
use proton_mail_html_transformer::Transformer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smart_default::SmartDefault;
use stash::orm::Model;
use stash::stash::Tether;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use super::MailboxResult;

/// What to do with the body. If in any of the fields `None` is specified it will read the relevant
/// value from the user setttings. If all are set, the db query will be elided.
#[derive(Debug, Clone, Copy, SmartDefault)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TransformOpts {
    #[default = true]
    pub show_block_quote: bool,
    pub hide_remote_images: Option<bool>,
    pub hide_embedded_images: Option<bool>,
    pub image_proxy: Option<bool>,
}

/// This is created after calling [`TransformOpts::fill_defaults`]
// It exists for type safety purposes.
#[derive(Debug, Clone, Copy)]
pub struct TransformOptsResolved<'a> {
    pub show_block_quote: bool,
    pub hide_remote_images: bool,
    pub hide_embedded_images: bool,
    pub image_proxy: Option<&'a AuthId>,
}

impl TransformOpts {
    /// Loads the relevant opts from the setttings.
    /// If all are set, the db query will be elided.
    #[must_use]
    pub async fn resolve<'a>(
        self,
        tether: &'_ Tether,
        auth_id: &'a AuthId,
    ) -> TransformOptsResolved<'a> {
        let show_block_quote = self.show_block_quote;
        if let (Some(hide_embedded_images), Some(hide_remote_images), Some(image_proxy)) = (
            self.hide_embedded_images,
            self.hide_remote_images,
            self.image_proxy,
        ) {
            return TransformOptsResolved {
                show_block_quote,
                hide_remote_images,
                hide_embedded_images,
                image_proxy: image_proxy.then_some(auth_id),
            };
        }

        let mail_settings = MailSettings::get_or_default(tether).await;
        let MailSettings {
            hide_remote_images,
            hide_embedded_images,
            image_proxy,
            ..
        } = mail_settings;

        TransformOptsResolved {
            show_block_quote,
            hide_remote_images: self.hide_remote_images.unwrap_or(hide_remote_images),
            hide_embedded_images: self.hide_embedded_images.unwrap_or(hide_embedded_images),
            image_proxy: self
                .image_proxy
                .unwrap_or(image_proxy | 2 == 2)
                .then_some(auth_id),
        }
    }
}

/// Which banners related to the body the client should show.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BodyBanners {
    /// Whether to show the "enable remote images" banner
    pub enable_show_remote_images: bool,
    /// Whether to show the "enable embedded images" banner
    pub enable_show_embedded_images: bool,
}

impl BodyBanners {
    fn new(opts: TransformOptsResolved<'_>) -> Self {
        Self {
            enable_show_remote_images: !opts.hide_remote_images,
            enable_show_embedded_images: !opts.hide_embedded_images,
        }
    }
}

impl From<TransformOptsResolved<'_>> for TransformOpts {
    fn from(val: TransformOptsResolved<'_>) -> Self {
        TransformOpts {
            show_block_quote: val.show_block_quote,
            hide_remote_images: Some(val.hide_remote_images),
            hide_embedded_images: Some(val.hide_embedded_images),
            image_proxy: Some(val.image_proxy.is_some()),
        }
    }
}

/// A message parsed header value can either be a string or an array of strings.
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

type InFlightAttachments = HashMap<LocalAttachmentId, JoinHandle<MailboxResult<Vec<u8>>>>;

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessageBody {
    /// The decrypted message contents.
    pub body: String,

    /// Metadata associated with the message body
    pub metadata: MessageBodyMetadata,

    /// Attachments that come from a multipart message.
    pub pgp_attachments: Option<Vec<ProcessedAttachment>>,

    /// The subject that comes from a multipart message.
    // TODO: Figure this out
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
    pub fn new(
        body: String,
        metadata: MessageBodyMetadata,
        pgp_attachments: Option<Vec<ProcessedAttachment>>,
        pgp_subject: Option<String>,
        ctx: Arc<MailUserContext>,
    ) -> Self {
        let in_flight = Mutex::new(Self::request_attachments(ctx, metadata.attachments.clone()));
        Self {
            body,
            metadata,
            pgp_attachments,
            pgp_subject,
            in_flight,
        }
    }

    fn request_attachments(
        ctx: Arc<MailUserContext>,
        atts: Vec<Attachment>,
    ) -> InFlightAttachments {
        atts.into_iter()
            .map(|att| {
                let id = att.id().unwrap();
                let ctx = ctx.clone();
                let fut = tokio::spawn(async move {
                    let data = ctx
                        .get_attachment_content_data(&att)
                        .await?
                        // We load this in the future so that it's there even if this has been cached before
                        .load()
                        .await?;
                    Ok(data)
                });
                (id, fut)
            })
            .collect()
    }

    pub async fn get_embedded_attachment(
        &self,
        ctx: &MailUserContext,
        cid: &str,
    ) -> MailboxResult<EmbeddedAttachmentInfo> {
        // We use this for logging if no embedded image was found.
        let mut available_cids = vec![];
        let mut cid_match = |x: &str| {
            // If the cid is provided in the `<foo@bar>` format
            let x = if x.starts_with('<') && x.ends_with('>') {
                &x[1..x.len() - 1]
            } else {
                // We leave this warning here to check if we need to support other cases in
                // the future.
                // TODO: remove me at some point.
                warn!("Weird cid format: {x}");
                x
            };

            if x == cid {
                true
            } else {
                available_cids.push(x.to_string());
                false
            }
        };

        let Some(att) = self
            .metadata
            .attachments
            .iter()
            // Notice that we don't check for the disposition, this is intentional.
            .find(|at| at.content_id.as_deref().is_some_and(&mut cid_match))
        else {
            // No correct cid found in the db... Let's check if it's a pgp attachment
            let find = self
                .pgp_attachments
                .as_ref()
                .and_then(|x| x.iter().find(|at| cid_match(&at.content_id)));
            match find {
                Some(at) => {
                    return Ok(EmbeddedAttachmentInfo {
                        data: at.data.clone(),
                        mime: at.mime_type.clone(),
                        height: None,
                        width: None,
                    })
                }
                None => {
                    return Err(AppError::UnknownCid(cid.to_string(), available_cids).into());
                }
            }
        };

        let data = {
            // We first remove the task from the mutex to avoid locking other threads.
            let task_handle = { self.in_flight.lock().remove(&att.id().unwrap()) };
            match task_handle {
                Some(p) => p.await.unwrap()?,
                None => ctx.get_attachment_content_data(att).await?.load().await?,
            }
        };
        Ok(EmbeddedAttachmentInfo {
            data,
            mime: att.mime_type.to_string(),
            height: att.image_height.clone(),
            width: att.image_width.clone(),
        })
    }
}

impl DecryptedMessageBody {
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
    /// * `ctx`            - Active mail user context.
    /// * `remote_content` - Controls behavior of remote content filtering.
    /// * `block_quote`    - Controls block quote behavior.
    ///
    /// # Errors
    ///
    /// Returns an error if the network request, the database query, reading/writing
    /// the body to the cache, or decrypting the body fails,
    /// or if the message doesn't exist.
    pub async fn transformed(&self, ctx: &MailUserContext, opts: TransformOpts) -> BodyOutput {
        let tether = ctx.user_stash().connection();
        let resolved = opts.resolve(&tether, ctx.session_id()).await;
        transform_html(&self.body, resolved, self.metadata.mime_type)
    }

    /// Create `DecryptedMessageBody` from a `StorableMessageBody` and a `MessageBodyMetadata`.
    pub(crate) fn from_storable(
        stored: StorableMessageBody,
        metadata: MessageBodyMetadata,
        ctx: Arc<MailUserContext>,
    ) -> Self {
        Self::new(
            stored.body,
            metadata,
            stored.pgp_attachments,
            stored.pgp_subject,
            ctx,
        )
    }
}

/// Consists of the message's body and decrypted content.
///
/// Used to store PGP attachments in cache along the message body.
///
#[derive(Default, Deserialize, Serialize)]
pub struct StorableMessageBody {
    /// The decrypted message contents.
    pub body: String,

    /// Attachments that come from a multipart message.
    pub pgp_attachments: Option<Vec<ProcessedAttachment>>,

    /// The subject that comes from a multipart message.
    // TODO: Figure this out
    pub pgp_subject: Option<String>,
}

impl StorableMessageBody {
    /// Serialize into a Vec<u8> with MessagePack format
    ///
    pub(crate) fn serialize(&self) -> Result<Vec<u8>, AppError> {
        Ok(rmp_serde::encode::to_vec(self)?)
    }

    /// Load a MessagePack encoded `DecryptedMessageBody` from a reader.
    ///
    pub fn from_reader(reader: impl Read) -> Result<Self, AppError> {
        Ok(rmp_serde::decode::from_read(reader)?)
    }
}

impl From<DecryptedMessageBody> for StorableMessageBody {
    fn from(value: DecryptedMessageBody) -> Self {
        Self {
            body: value.body,
            pgp_attachments: value.pgp_attachments,
            pgp_subject: value.pgp_subject,
        }
    }
}

/// Consists of the message's body and decrypted content.
///
/// Used to store PGP attachments in cache along the message body.
///
#[derive(Default, Serialize)]
pub struct StorableMessageBodyRef<'r> {
    /// The decrypted message contents.
    pub body: &'r str,

    /// Attachments that come from a multipart message.
    pub pgp_attachments: Option<&'r [ProcessedAttachment]>,

    /// The subject that comes from a multipart message.
    // TODO: Figure this out
    pub pgp_subject: Option<&'r str>,
}

impl<'r> StorableMessageBodyRef<'r> {
    /// Create a new instance
    pub(crate) fn from_decrypted_message_body(value: &'r DecryptedMessageBody) -> Self {
        Self {
            body: value.body.as_str(),
            pgp_attachments: value.pgp_attachments.as_deref(),
            pgp_subject: value.pgp_subject.as_deref(),
        }
    }

    /// Serialize into a Vec<u8> with MessagePack format
    ///
    pub(crate) fn serialize(&self) -> Result<Vec<u8>, AppError> {
        Ok(rmp_serde::encode::to_vec(self)?)
    }
}

/// The result of transforming the message body.
/// It will have more things in the future
#[non_exhaustive]
#[derive(Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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

    /// How many images it has proxied.
    pub images_proxied: u64,

    /// The transform opts that were used. All fields are actually Some.
    pub transform_opts: TransformOpts,

    /// This instructs the client on what banners they should show.
    pub body_banners: BodyBanners,
}

impl std::fmt::Debug for BodyOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BodyOutput")
            .field("body", &(format!("{} bytes", self.body.len())))
            .field("had_blockquote", &self.had_blockquote)
            .field("tags_stripped", &self.tags_stripped)
            .field("utm_stripped", &self.utm_stripped)
            .field("remote_images_disabled", &self.remote_images_disabled)
            .field("embedded_images_disabled", &self.embedded_images_disabled)
            .field("images_proxied", &self.images_proxied)
            .field("transform_opts", &self.transform_opts)
            .field("body_banners", &self.body_banners)
            .finish()
    }
}

#[tracing::instrument(skip(html))]
pub fn transform_html(
    html: &str,
    opts: TransformOptsResolved<'_>,
    mime_type: MimeType,
) -> BodyOutput {
    // The order at which we run the transforms is not random, it's been chosen for maximum
    // efficiency.

    let TransformOptsResolved {
        show_block_quote,
        hide_remote_images,
        hide_embedded_images,
        image_proxy,
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

    let embedded_images_disabled = if hide_embedded_images {
        transformer.disable_embedded_images()
    } else {
        0
    };

    let mut remote_images_disabled = 0;
    let mut images_proxied = 0;
    if hide_remote_images {
        remote_images_disabled = transformer.disable_remote_content();
    } else if let Some(auth_id) = image_proxy {
        // Doesn't make sense to proxy images if they have been disabled ;)

        // FIXME: https://protonmail.slack.com/archives/C02EQ2TDNQM/p1736178345208839
        if false {
            images_proxied = transformer.proxy_images(auth_id.as_ref());
        }
    }

    let had_blockquote = if !show_block_quote {
        transformer.strip_blockquote()
    } else {
        false
    };

    if cfg!(target_os = "ios") {
        transformer.inject_ios_content_size();
    }

    transformer.inject_style();

    let output = BodyOutput {
        body: transformer.to_string(),
        had_blockquote,
        tags_stripped,
        utm_stripped,
        remote_images_disabled,
        embedded_images_disabled,
        images_proxied,
        transform_opts: opts.into(),
        body_banners: BodyBanners::new(opts),
    };
    debug!("Transform done. Output: {output:#?}");
    output
}
