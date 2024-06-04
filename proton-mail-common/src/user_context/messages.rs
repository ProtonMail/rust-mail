use crate::db::{LocalMessageBodyMetadata, LocalMessageId, LocalMessageMetadata};
use crate::exports::tracing;
use crate::exports::tracing::{error, Level};
use crate::{MailContextError, MailContextResult, MailUserContext, MailboxError, MailboxResult};
use proton_api_mail::domain::MessageMetadataFilter;
use proton_api_mail::exports::anyhow::anyhow;
use std::path::PathBuf;

impl MailUserContext {
    /// Synchronize the message body for `message_id`.
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_message_body(
        &self,
        message_id: LocalMessageId,
    ) -> MailboxResult<LocalMessageBodyMetadata> {
        // TODO(ET-231): Use caching solution.
        let metadata = if let Some(metadata) = self
            .db_read(|conn| conn.message_body(message_id))
            .map_err(|e| {
                error!("Failed to retrieve message body metadata from db: {e}");
                e
            })? {
            metadata
        } else {
            // metadata is not there it is either missing or the message does not exist.
            let Some(Some(remote_id)) = self.db_read(|conn| conn.message_remote_id(message_id))?
            else {
                return Err(MailboxError::MessageDoesNotHaveRemoteId(message_id));
            };

            // sync the message body
            let message = self.mail_session().message(&remote_id).await.map_err(|e| {
                error!("Failed to retrieve message: {e}");
                MailContextError::from(e)
            })?;

            let cache_path = self.message_cache_path(message_id);
            // create message in the database and store body in the cache.
            self.db_write(|tx| -> MailboxResult<LocalMessageBodyMetadata> {
                let metadata = tx.create_or_update_message_body(&message).map_err(|e| {
                    error!("Failed to store message body in db: {e}");
                    e
                })?;

                // TODO(ET-231): Write to cache.
                std::fs::write(&cache_path, &message.body).map_err(|e| {
                    error!("Failed to write message body: {e}");
                    MailboxError::Context(MailContextError::Other(anyhow!("{e}")))
                })?;

                Ok(metadata)
            })?
        };

        Ok(metadata)
    }

    /// Get the cache path for a message body with `id`.
    pub fn message_cache_path(&self, id: LocalMessageId) -> PathBuf {
        self.mail_context()
            .mail_cache_path()
            .join(format!("message_body_{id}"))
    }

    /// Filter or Search messages which match the given `filter`.
    ///
    /// Note that search results are inserted into the database.
    ///
    /// # Errors
    /// Returns error if the network request or the query failed.
    pub async fn filter_messages(
        &self,
        filter: MessageMetadataFilter,
    ) -> MailContextResult<FilteredMessages> {
        let response = self.mail_session().message_metadata(filter).await?;

        let messages = if !response.messages.is_empty() {
            self.db_write(|tx| {
                let ids = tx.create_messages_from_metadata(response.messages.iter())?;
                tx.get_messages_metadata(ids.into_iter())
            })?
        } else {
            Vec::new()
        };

        Ok(FilteredMessages {
            total: response.total,
            messages,
        })
    }
}

/// Result of the call to [`MailUserContext::filter_messages`].
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FilteredMessages {
    /// Total number of message that match the filter.
    pub total: u64,
    /// Returned messages that match the filter.
    pub messages: Vec<LocalMessageMetadata>,
}
