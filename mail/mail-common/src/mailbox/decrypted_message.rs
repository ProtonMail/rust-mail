#![allow(dead_code)]

//! Everything related to processing a decrypted message.

use crate::datatypes::MimeType;
use crate::models::{Attachment, EmbeddedAttachmentInfo, MailSettings, MessageBodyMetadata};
use crate::{AppError, MailUserContext, MailboxError};
use parking_lot::Mutex;
use proton_core_common::datatypes::LocalId;
use proton_crypto_inbox::proton_crypto_inbox_mime::ProcessedAttachment;
use proton_mail_html_transformer::Transformer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stash::orm::Model;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::warn;

use super::MailboxResult;

/// Enable or disable remote content (images).
/// The default behaviour is Default.
#[derive(Debug, Clone, Copy, Default)]
pub enum RemoteContent {
    #[default]
    Default, // Use whatever is in the user's [`MailSettings`]
    Enabled,  // Override the settings and show images
    Disabled, // Override the settings and don't show images
}

/// What to do with the blockquote (previous conversation threads)
/// The default behaviour is Strip.
#[derive(Debug, Clone, Copy, Default)]
pub enum BlockQuote {
    #[default]
    Strip,
    Untouched,
}

/// A message parsed header value can either be a string or an array of strings.
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

type InFlightAttachments = HashMap<LocalId, JoinHandle<MailboxResult<Vec<u8>>>>;

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
    pub async fn transformed(
        &self,
        ctx: &MailUserContext,
        remote_content: RemoteContent,
        block_quote: BlockQuote,
    ) -> Result<BodyOutput, MailboxError> {
        let tether = ctx.user_stash().connection();
        let mail_settings = MailSettings::get_or_default(&tether).await;
        let user_session_id = ctx.user_id();
        let BodyOutput {
            body,
            had_blockquote,
            tags_stripped,
            utm_stripped,
        } = transform_html(
            &self.body,
            remote_content,
            block_quote,
            &mail_settings,
            user_session_id,
            self.metadata.mime_type,
        );
        Ok(BodyOutput {
            body,
            had_blockquote,
            tags_stripped,
            utm_stripped,
        })
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

/// The result of transforming the message body.
pub struct BodyOutput {
    /// The transformed html of the message.
    pub body: String,

    /// Whether or not [`RemoteContent::Strip`] removed a blockquote.
    pub had_blockquote: bool,

    /// How many html tags it has removed.
    pub tags_stripped: u64,

    /// How many UTM tracking params it has removed.
    pub utm_stripped: u64,
}

pub fn transform_html(
    html: &str,
    remote_content: RemoteContent,
    blockquote: BlockQuote,
    mail_settings: &MailSettings,
    user_session_id: &str,
    mime_type: MimeType,
) -> BodyOutput {
    // If the message is text/plain we need to apply some extra transforms to it like
    // preserving whitespaces and adding links.
    let mut transformer = if mime_type == MimeType::TextPlain {
        let mut transformer = Transformer::new_text_plain(html);
        transformer.add_noreferrer();
        transformer.insert_links();
        transformer
    } else {
        let mut transformer = Transformer::new(html);
        transformer.add_noreferrer();
        transformer
    };
    let tags_stripped = transformer.strip_whitelist();
    let utm_stripped = transformer.strip_utm();

    transformer.inject_style();

    if mail_settings.image_proxy | 2 == 2 {
        transformer.proxy_images(user_session_id);
    }

    if cfg!(target_os = "ios") {
        transformer.inject_ios_content_size();
    }

    match remote_content {
        RemoteContent::Disabled | // Explicit disable
        RemoteContent::Default if mail_settings.hide_remote_images  => {
            transformer.disable_remote_content();
        }
        _ => (),
    }

    let had_blockquote = if let BlockQuote::Strip = blockquote {
        transformer.strip_blockquote()
    } else {
        false
    };

    BodyOutput {
        body: transformer.to_string(),
        had_blockquote,
        tags_stripped,
        utm_stripped,
    }
}
