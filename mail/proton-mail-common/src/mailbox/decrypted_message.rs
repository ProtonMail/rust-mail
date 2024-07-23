//! Everything related to processing a decrypted message.

use crate::db::serde_json::Value;
use crate::db::LocalMessageBodyMetadata;
use crate::exports::{thiserror, tracing};
use proton_api_mail::domain::{MailSettings, MimeType};
use proton_crypto_inbox::message::DecryptedBody;
use proton_mail_html_transformer::Transformer;

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessage {
    /// Metadata associated with the message body
    metadata: LocalMessageBodyMetadata,
    /// The decrypted message contents.
    body: Type,
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptedMessageError {
    #[error("Body type is not valid for this operation")]
    InvalidBodyType,
    #[error("Html Tansformer: {0}")]
    Transform(#[from] proton_mail_html_transformer::Error),
}
/// Type of the encrypted message.
enum Type {
    /// Plain text.
    Text(String),
    /// Html
    Html(HtmlMessage),
}

/// Html body contains the [`Transformer`] and the cached results.
struct HtmlMessage {
    /// Html Transformer which contains the parsed document.
    transformer: Transformer,
    /// Cached HTML output.
    body: String,
    /// Whether remote content is enabled or not.
    remote_content_enabled: bool,
}

impl HtmlMessage {
    fn new(mail_settings: &MailSettings, body: String) -> Result<Self, DecryptedMessageError> {
        let mut transformer = Transformer::new(&body);
        transformer
            .strip_utm()
            .map_err(proton_mail_html_transformer::Error::from)?;

        #[cfg(target_os = "ios")]
        transformer.inject_ios_content_size()?;

        if mail_settings.hide_remote_images {
            transformer
                .disable_remote_content()
                .map_err(proton_mail_html_transformer::Error::from)?;
        }

        let body = transformer.to_string();

        Ok(Self {
            transformer,
            body,
            remote_content_enabled: !mail_settings.hide_remote_images,
        })
    }

    /// Re-enable HTML remote content embedded in the message.
    ///
    /// # Errors
    ///
    /// Returns error if the process fails.
    fn enable_remote_content(&mut self) -> Result<(), DecryptedMessageError> {
        if !self.remote_content_enabled {
            return Ok(());
        }

        self.with_transformer(|t| {
            Ok(t.enable_remote_content()
                .map_err(proton_mail_html_transformer::Error::from)?)
        })?;

        self.remote_content_enabled = true;

        Ok(())
    }

    /// Disable HTML remote content embedded in the message.
    ///
    /// # Errors
    ///
    /// Returns error if the process fails.
    fn disable_remote_content(&mut self) -> Result<(), DecryptedMessageError> {
        if self.remote_content_enabled {
            return Ok(());
        }

        self.with_transformer(|t| {
            Ok(t.disable_remote_content()
                .map_err(proton_mail_html_transformer::Error::from)?)
        })?;

        self.remote_content_enabled = false;
        Ok(())
    }

    /// Utility function that regenerates the HTML body after interacting with the transformer.
    fn with_transformer(
        &mut self,
        closure: impl FnOnce(&mut Transformer) -> Result<(), DecryptedMessageError>,
    ) -> Result<(), DecryptedMessageError> {
        closure(&mut self.transformer)?;
        self.body = self.transformer.to_string();
        Ok(())
    }
}

/// A message parsed header value can either be a string or an array of strings.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

impl DecryptedMessage {
    /// Process a `decrypted_body` into a displayable HTML message.
    ///
    /// The `mail_settings` are required to identify the default transformation passes that should
    /// be applied to the message.
    pub fn new(
        mail_settings: &MailSettings,
        metadata: LocalMessageBodyMetadata,
        decrypted_body: DecryptedBody,
    ) -> Result<Self, DecryptedMessageError> {
        let body = match decrypted_body {
            DecryptedBody::Plain(body) => body,
            DecryptedBody::Mime(multipart) => {
                //TODO(ET-263): Handle multipart messages.
                multipart.body
            }
        };

        if !matches!(metadata.mime_type, MimeType::TextHTML) {
            return Ok(Self {
                metadata,
                body: Type::Text(body),
            });
        }

        Ok(Self {
            metadata,
            body: Type::Html(HtmlMessage::new(mail_settings, body)?),
        })
    }

    /// Retrieve a parsed header value for a given `key`.
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        let value = self.metadata.parsed_headers.get(key)?;
        match value {
            Value::String(s) => Some(ParsedHeaderValue::String(s.clone())),
            Value::Array(array) => {
                let mut result = Vec::with_capacity(array.len());
                for (idx, item) in array.iter().enumerate() {
                    if let Value::String(str) = item {
                        result.push(str.clone());
                    } else {
                        tracing::warn!(
                            "Header array value {key}[{idx}] of message {} has invalid value type",
                            self.metadata.id
                        );
                    }
                }
                Some(ParsedHeaderValue::Array(result))
            }
            _ => {
                tracing::warn!(
                    "Header value {key} of message {} has invalid value type",
                    self.metadata.id
                );
                None
            }
        }
    }

    /// Access the message's body.
    #[inline]
    pub fn body(&self) -> &str {
        match &self.body {
            Type::Text(body) => body.as_str(),
            Type::Html(body) => body.body.as_str(),
        }
    }

    /// Access the message's body metadata.
    #[inline]
    pub fn metadata(&self) -> &LocalMessageBodyMetadata {
        &self.metadata
    }

    /// Enable remote images.
    ///
    /// # Errors
    ///
    /// Returns error if the process failed.
    pub fn enable_remote_images(&mut self) -> Result<(), DecryptedMessageError> {
        let Type::Html(html) = &mut self.body else {
            // can not be applied to plain text messages.
            return Err(DecryptedMessageError::InvalidBodyType);
        };

        html.enable_remote_content()
    }

    /// Disable remote images.
    ///
    /// # Errors
    ///
    /// Returns error if the process failed.
    pub fn disable_remote_images(&mut self) -> Result<(), DecryptedMessageError> {
        let Type::Html(html) = &mut self.body else {
            // can not be applied to plain text messages.
            return Err(DecryptedMessageError::InvalidBodyType);
        };

        html.disable_remote_content()
    }
}
