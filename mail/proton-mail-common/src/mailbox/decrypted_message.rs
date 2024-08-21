#![allow(dead_code)]

//! Everything related to processing a decrypted message.

use crate::models::{MailSettings, MessageBodyMetadata};
use proton_mail_html_transformer::Transformer;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default)]
pub enum RemoteContent {
    #[default]
    Default, // Use whatever is in the user's [`MailSettings`]
    Enabled,  // Override the settings and show images
    Disabled, // Override the settings and don't show images
}

/// What to do with the blockquote (previous conversation threads)
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
) -> BodyOutput {
    let mut transformer = Transformer::new(html);
    let tags_stripped = transformer.strip_whitelist();
    let utm_stripped = transformer.strip_utm();

    transformer.add_noreferrer();
    transformer.insert_links();
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
