use crate::db::{LocalMessageBodyMetadata, LocalMessageId, LocalMessageMetadata, MessageQuery};
use crate::exports::crypto::proton_crypto::new_pgp_provider;
use crate::exports::tracing::error;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::domain::{MailSettingsViewMode, MimeType};
use proton_api_mail::exports::anyhow::anyhow;
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};

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
            DecryptedBody::Plain(body) => Ok(DecryptedMessageBody {
                metadata: encrypted_msg.metadata,
                body,
            }),
            DecryptedBody::Mime(multipart) => {
                //TODO(ET-263): Handle multipart messages.
                Ok(DecryptedMessageBody {
                    metadata: encrypted_msg.metadata,
                    body: multipart.body,
                })
            }
        }
    }
}

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessageBody {
    /// Metadata associated with the message body
    pub metadata: LocalMessageBodyMetadata,
    /// The decrypted message contents.
    pub body: String,
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
