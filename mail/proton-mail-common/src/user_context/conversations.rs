use crate::db::{DBResult, LocalConversation, LocalConversationId, LocalLabelId};
use crate::exports::tracing::error;
use crate::{MailContextError, MailContextResult, MailUserContext, MailboxError, MailboxResult};
use proton_api_mail::domain::{ConversationFilterBuilder, LabelId, MessageMetadataFilterBuilder};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};

impl MailUserContext {
    /// Synchronize the first `count` conversations of the label with `label_id`.
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_first_conversation_page(
        &self,
        label_id: LabelId,
        count: usize,
    ) -> MailContextResult<()> {
        let session = self.mail_session();
        let filter = ConversationFilterBuilder::new(0, count)
            .with_label_id(label_id)
            .descending()
            .build();
        let response = session.conversations(filter).await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        self.db_write(|tx| tx.create_conversations(response.conversations.iter()))?;
        Ok(())
    }

    /// Synchronize the first `count` messages of the label with `label_id`.
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_first_message_page(
        &self,
        label_id: LabelId,
        count: usize,
    ) -> MailContextResult<()> {
        let session = self.mail_session();
        let filter = MessageMetadataFilterBuilder::new(0, count)
            .with_label_id(label_id)
            .descending()
            .build();
        let response = session.message_metadata(filter).await?;

        debug!(
            "Fetched {} messages TOTAL={}",
            response.messages.len(),
            response.total
        );

        self.db_write(|tx| tx.create_messages_from_metadata(response.messages.iter()))?;
        Ok(())
    }

    /// Synchronize the conversations and message counts for each label.
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    pub async fn sync_conversation_and_message_counts(&self) -> MailContextResult<()> {
        let conversation_counts = self.mail_session().conversation_counts().await?;
        let message_counts = self.mail_session().message_counts().await?;

        let mut connection = self.new_db_connection()?;
        connection.tx(|tx| -> DBResult<()> {
            tx.create_or_update_conversation_counts(conversation_counts.iter())?;
            tx.create_or_update_message_counts(message_counts.iter())?;
            Ok(())
        })?;
        Ok(())
    }

    /// Sync the conversation with `id`'s messages.
    ///
    /// If this is the first time this is called, the messages will be downloaded from the server.
    ///
    /// # Errors
    /// Returns error if the db queries failed or the network request failed.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_conversation_messages(&self, id: LocalConversationId) -> MailboxResult<()> {
        let Some((has_messages, rid)) = self
            .db_read(|conn| conn.conversation_has_messages(id))
            .map_err(MailContextError::from)?
        else {
            return Err(MailboxError::ConversationNotFound(id));
        };

        if !has_messages {
            let Some(rid) = rid else {
                return Err(MailboxError::ConversationDoesNotHaveRemoteId(id));
            };
            debug!("Syncing conversation messages");
            let conversation = self.mail_session().conversation(&rid).await.map_err(|e| {
                error!("failed to download conversation messages: {e}");
                MailContextError::from(e)
            })?;

            self.db_write(|tx| {
                tx.create_messages_from_metadata(conversation.messages.iter())?;
                tx.set_conversation_has_messages(id, true)
            })
            .map_err(|e| {
                error!("Failed to write message metadata: {e}");
                MailContextError::DB(e)
            })?;
        }

        Ok(())
    }

    /// Get `count` conversations for `local_label_id`.
    ///
    /// # Errors
    /// Returns error data could not be retrieved from the database.
    pub fn conversations_with_context_for_label(
        &self,
        local_label_id: LocalLabelId,
        count: usize,
    ) -> MailContextResult<Vec<LocalConversation>> {
        let connection = self.new_db_connection()?;
        Ok(connection.read(|conn| conn.get_conversations_with_context(local_label_id, count))?)
    }
}
