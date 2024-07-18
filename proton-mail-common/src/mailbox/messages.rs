use crate::db::serde_json::Value;
use crate::db::{LocalMessageBodyMetadata, LocalMessageId, LocalMessageMetadata, MessageQuery};
use crate::exports::crypto::proton_crypto::new_pgp_provider;
use crate::exports::tracing;
use crate::exports::tracing::error;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::domain::{MailSettingsViewMode, MimeType};
use proton_api_mail::exports::anyhow::anyhow;
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};
use proton_mail_html_transformer::{Options, Transformer};

impl Mailbox {
    /// Create a new live query for messages.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Messages`].
    pub fn new_messages_query<Builder: MailboxObservableQueryBuilder<MessageQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Result<Builder::Output, MailboxError> {
        if self.view_mode() != MailSettingsViewMode::Messages {
            error!(
                "Mailbox is not in message view, current view mode = {:?}",
                self.view_mode()
            );
            return Err(MailboxError::InvalidViewMode);
        }

        Ok(builder.build(
            self.user_ctx.tracker_service().clone(),
            MessageQuery::new(self.label_id, limit),
        ))
    }

    /// Get up to `count` messages in this mailbox.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn messages(&self, count: usize) -> MailboxResult<Vec<LocalMessageMetadata>> {
        Ok(self
            .user_ctx
            .db_read(|conn| conn.message_metadata_list(self.label_id, count))
            .map_err(MailContextError::DB)?)
    }

    /// Get the message's body.
    ///
    /// This will attempt to fetch the message data from the servers if it has not yet been
    /// downloaded before.
    ///
    /// # Errors
    /// Returns error if the message failed to download, the db query failed or the message
    /// body could not be written to the cache.
    pub async fn message_body(&self, id: LocalMessageId) -> MailboxResult<DecryptedMessageBody> {
        // Fetch metadata first to sync contents and cache.
        let metadata = self.user_ctx.sync_message_body(id).await?;

        let cache_path = self.user_ctx.message_cache_path(id);

        // TODO(ET-231): Read body from cache.
        let encrypted_body = std::fs::read_to_string(cache_path).map_err(|e| {
            error!("Failed to read encrypted message body from cache: {e}");
            MailboxError::Context(MailContextError::Other(anyhow!("{e}")))
        })?;

        // Decrypt message.

        let encrypted_msg = EncryptedMessageBody {
            metadata,
            encrypted_body,
        };

        let pgp_provider = new_pgp_provider();

        // get address key
        let address_keys = self
            .user_ctx
            .unlocked_address_keys_async(&pgp_provider, &encrypted_msg.metadata.address_id)
            .await
            .map_err(|e| {
                error!(
                    "Failed to unlock address keys ID={}: {e}",
                    encrypted_msg.metadata.address_id
                );
                e
            })?;

        //TODO: Verify signature.
        let (decrypted_body, _) = encrypted_msg
            .decrypt(&pgp_provider, &address_keys)
            .map_err(|e| {
                error!("Failed to decrypt message ({id}): {e}");
                e
            })?;

        match decrypted_body {
            DecryptedBody::Plain(body) => {
                Ok(DecryptedMessageBody::new(encrypted_msg.metadata, body)?)
            }
            DecryptedBody::Mime(multipart) => {
                //TODO(ET-263): Handle multipart messages.
                Ok(DecryptedMessageBody::new(
                    encrypted_msg.metadata,
                    multipart.body,
                )?)
            }
        }
    }
}

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessageBody {
    /// Metadata associated with the message body
    metadata: LocalMessageBodyMetadata,
    /// The decrypted message contents.
    body: String,
}

/// A message parsed header value can either be a string or an array of strings.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

impl DecryptedMessageBody {
    pub fn new(
        metadata: LocalMessageBodyMetadata,
        body: String,
    ) -> Result<Self, proton_mail_html_transformer::Error> {
        if !matches!(metadata.mime_type, MimeType::TextHTML) {
            return Ok(Self { metadata, body });
        }
        // TODO(ET-384): Preserve original html string and parsed result
        // so it can be modified on demand without having to reparse.
        let options = Options {
            strip_utm: true,
            #[cfg(target_os = "ios")]
            inject_ios_content_size: true,
            #[cfg(not(target_os = "ios"))]
            inject_ios_content_size: false,
            ..Default::default()
        };

        let transformer = Transformer::new(options);
        let body = transformer.transform(&body)?.to_string();

        Ok(Self { metadata, body })
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
        &self.body
    }

    /// Access the message's body metadata.
    #[inline]
    pub fn metadata(&self) -> &LocalMessageBodyMetadata {
        &self.metadata
    }
}

struct EncryptedMessageBody {
    pub metadata: LocalMessageBodyMetadata,
    pub encrypted_body: String,
}

impl DecryptableMessage for EncryptedMessageBody {
    fn message_id(&self) -> Option<&str> {
        self.metadata.rid.as_ref().map(|v| v.as_ref())
    }

    fn message_is_mime(&self) -> bool {
        self.metadata.mime_type == MimeType::MultipartMixed
    }

    fn message_encrypted_body(&self) -> &[u8] {
        self.encrypted_body.as_bytes()
    }
}
