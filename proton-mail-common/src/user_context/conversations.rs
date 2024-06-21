use crate::db::{DBResult, LocalConversation, LocalConversationId, LocalLabelId};
use crate::exports::anyhow::anyhow;
use crate::exports::tracing::error;
use crate::{MailContextError, MailContextResult, MailUserContext, MailboxError, MailboxResult};
use proton_api_mail::domain::{
    ConversationFilter, ConversationFilterBuilder, ConversationId, LabelId,
    MessageMetadataFilterBuilder,
};
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
        } else {
            debug!("Conversation messages already synced")
        }

        Ok(())
    }

    /// Sync conversation with `id`.
    ///
    /// If this is the first time this is called, the conversation will be downloaded from the
    /// server.
    ///
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_conversation(&self, id: LocalConversationId) -> MailboxResult<()> {
        let (is_known, rid) = self
            .db_read(|conn| conn.is_conversation_known(id))
            .map_err(MailContextError::from)?;
        if is_known {
            debug!("Conversation is known, syncing messages only");
            // if known sync messages.
            return self.sync_conversation_messages(id).await;
        }

        let Some(rid) = rid else {
            return Err(MailboxError::ConversationDoesNotHaveRemoteId(id));
        };

        debug!("Conversation is not known, syncing");
        self.sync_conversation_impl(&rid).await
    }

    /// Sync conversation with remote `id`.
    ///
    /// If this is the first time this is called, the conversation will be downloaded from the
    /// server.
    ///
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_conversation_with_remote_id(&self, id: &ConversationId) -> MailboxResult<()> {
        let is_known = self
            .db_read(|conn| conn.is_conversation_known_with_remote_id(id))
            .map_err(MailContextError::from)?;
        if is_known {
            debug!("Conversation is known, syncing messages only");
            // if known sync messages.
            let Some(local_id) = self.db_read(|conn| conn.conversation_id_from_remote_id(id))?
            else {
                return Err(MailContextError::Other(anyhow!(
                    "Failed to find conversation with remote id {id}"
                ))
                .into());
            };
            return self.sync_conversation_messages(local_id).await;
        }

        debug!("Conversation is not known, syncing");
        self.sync_conversation_impl(id).await
    }

    async fn sync_conversation_impl(&self, id: &ConversationId) -> MailboxResult<()> {
        let conversation = self.mail_session().conversation(id).await.map_err(|e| {
            error!("failed to download conversation: {e}");
            MailContextError::from(e)
        })?;

        self.db_write(|tx| {
            let conv_id = tx.create_conversation(&conversation.conversation)?;
            tx.create_messages_from_metadata(conversation.messages.iter())?;
            tx.set_conversation_has_messages(conv_id, true)
        })
        .map_err(|e| {
            error!("Failed to create conversation: {e}");
            e
        })?;

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

    /// Filter or Search conversations which match the given `filter`.
    ///
    /// Note that search results are inserted into the database.
    ///
    /// # Errors
    /// Returns error if the network request or the query failed.
    pub async fn filter_conversations(
        &self,
        filter: ConversationFilter,
    ) -> MailContextResult<FilteredConversations> {
        let response = self.mail_session().conversations(filter).await?;

        let conversations = if !response.conversations.is_empty() {
            self.db_write(|tx| {
                let ids = tx.create_conversations(response.conversations.iter())?;
                tx.get_conversations(ids.into_iter())
            })?
        } else {
            Vec::new()
        };

        Ok(FilteredConversations {
            total: response.total,
            conversations,
        })
    }

    /// Retrieve a conversation by `id` in the All Mail context.
    ///
    /// If the conversation does not exist, it will be retrieved from the server.
    ///
    /// # Errors
    /// Returns error if the db query or the network request failed.
    pub async fn conversation_with_id_with_all_mail_context(
        &self,
        id: LocalConversationId,
    ) -> MailboxResult<Option<LocalConversation>> {
        self.sync_conversation(id).await?;
        Ok(self.db_read(|conn| {
            // ALL Mail label is always there, this unlikely to fail.
            let Some(label_id) = conn.resolve_remote_label_id(LabelId::all_mail())? else {
                return Ok(None);
            };
            conn.get_conversation_with_context(id, label_id)
        })?)
    }

    /// Retrieve a conversation by remote `id` in the All Mail context.
    ///
    /// If the conversation does not exist, it will be retrieved from the server.
    ///
    /// # Errors
    /// Returns error if the db query or the network request failed.
    pub async fn conversation_with_remote_id(
        &self,
        id: &ConversationId,
    ) -> MailboxResult<Option<LocalConversation>> {
        self.sync_conversation_with_remote_id(id).await?;

        Ok(self.db_read(|conn| {
            let Some(conv_id) = conn.conversation_id_from_remote_id(id)? else {
                return Ok(None);
            };
            // ALL Mail label is always there, this unlikely to fail.
            let Some(label_id) = conn.resolve_remote_label_id(LabelId::all_mail())? else {
                return Ok(None);
            };
            conn.get_conversation_with_context(conv_id, label_id)
        })?)
    }

    /// Retrieve a conversation by `id` in the `label_id` context.
    ///
    /// If the conversation does not exist, it will be retrieved from the server.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub async fn conversation_with_id_and_context(
        &self,
        id: LocalConversationId,
        label_id: LocalLabelId,
    ) -> MailboxResult<Option<LocalConversation>> {
        self.sync_conversation(id).await?;

        Ok(self.db_read(|conn| conn.get_conversation_with_context(id, label_id))?)
    }
}

/// Result of the call to [`MailUserContext::filter_conversations`].
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct FilteredConversations {
    /// Total number of conversations that match the filter.
    pub total: u64,
    /// Returned conversations that match the filter.
    pub conversations: Vec<LocalConversation>,
}
