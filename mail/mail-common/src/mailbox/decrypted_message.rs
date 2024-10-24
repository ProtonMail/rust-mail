#![allow(dead_code)]

//! Everything related to processing a decrypted message.

use crate::datatypes::MimeType;
use crate::models::{MailSettings, MessageBodyMetadata};
use crate::{MailUserContext, MailboxError};
use proton_crypto_inbox::proton_crypto_inbox_mime::ProcessedAttachment;
use proton_mail_html_transformer::Transformer;
use serde_json::Value;

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

/// Consists of the message's body metadata and decrypted content.
#[derive(Clone, Debug, Eq, PartialEq)]
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
        let mail_settings = MailSettings::get_or_default(ctx.user_stash()).await;
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

    // Only insert links if message is of type text.
    if mime_type == MimeType::TextPlain {
        transformer.insert_links();
    }

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
