use crate::actions::{
    DeleteConversationsAction, LabelConversationsAction, MarkConversationsReadAction,
    MarkConversationsUnreadAction, MoveConversationsAction, UnlabelConversationsAction,
};
use crate::db::{
    ConversationMessagesQuery, ConversationQuery, LocalConversation, LocalConversationId,
    LocalLabelId, LocalMessageMetadata,
};
use crate::exports::anyhow::anyhow;
use crate::exports::tracing::error;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::domain::{LabelId, MailSettingsViewMode};

impl Mailbox {
    /// Create a new live query for conversations.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Conversations`].
    pub fn new_conversation_query<Builder: MailboxObservableQueryBuilder<ConversationQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Result<Builder::Output, MailboxError> {
        if self.view_mode() != MailSettingsViewMode::Conversations {
            error!(
                "Mailbox is not in conversation view, current view mode = {:?}",
                self.view_mode()
            );
            return Err(MailboxError::InvalidViewMode);
        }

        Ok(builder.build(
            self.user_ctx.tracker_service().clone(),
            ConversationQuery::new(self.label_id, limit),
        ))
    }

    pub fn conversations(&self, count: usize) -> MailboxResult<Vec<LocalConversation>> {
        let v = self
            .user_ctx
            .conversations_with_context_for_label(self.label_id, count)?;
        Ok(v)
    }

    pub fn delete_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(DeleteConversationsAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn mark_conversations_read(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MarkConversationsReadAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn mark_conversations_unread(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MarkConversationsUnreadAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn label_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(LabelConversationsAction::new(label_id, ids))?;
        Ok(())
    }

    pub fn unlabel_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(UnlabelConversationsAction::new(label_id, ids))?;
        Ok(())
    }

    /// Star a conversation. This is the equivalent of adding a conversation to the Starred system
    /// label.
    ///
    /// # Error
    /// Return error if the operation failed.
    pub fn star_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let label_id = self.starred_label_id()?;
        self.label_conversations(label_id, ids)
    }

    /// Unstar a conversation. This is the equivalent of removing a conversation from the Starred
    /// system label.
    ///
    /// # Error
    /// Return error if the operation failed.
    pub fn unstar_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let label_id = self.starred_label_id()?;
        self.unlabel_conversations(label_id, ids)
    }

    /// Move conversations to a given folder.
    pub fn move_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MoveConversationsAction::new(self.label_id, label_id, ids))?;
        Ok(())
    }

    /// Move conversations to a given folder with a `remote_id`.
    ///
    /// # Errors
    /// Return error if the action failed, the `remote_id` does not exist or the label
    /// is not a valid destination.
    pub fn move_conversations_with_remote_id(
        &self,
        remote_id: &LabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let Some(label) = self
            .user_ctx
            .db_read(|conn| conn.label_with_remote_id(remote_id))
            .map_err(MailContextError::from)?
        else {
            return Err(MailboxError::RemoteLabelNotFound(remote_id.clone()));
        };
        if !label.is_movable_folder() {
            return Err(MailboxError::InvalidAction(anyhow!(
                "Destination is not a valid folder"
            )));
        }
        self.user_ctx
            .queue_action(MoveConversationsAction::new(self.label_id, label.id, ids))?;
        Ok(())
    }

    /// Retrieve the conversation with `id`'s messages.
    ///
    /// If this is the first time this is called, the messages will be downloaded from the server.
    ///
    /// # Errors
    /// Returns error if the db queries failed or the network request failed.
    pub async fn conversation_messages(
        &self,
        id: LocalConversationId,
    ) -> MailboxResult<Vec<LocalMessageMetadata>> {
        self.user_context().sync_conversation_messages(id).await?;
        Ok(self
            .user_ctx
            .db_read(|conn| conn.messages_metadata_for_conversation(id))
            .map_err(MailContextError::from)?)
    }

    /// Create a new live query for a conversation with `id` 's messages.
    ///
    /// If this is the first time this is called, the messages will be downloaded from the server.
    ///
    /// # Errors
    /// Return error if the network request or the database operation failed.
    pub async fn new_conversation_message_query<
        Builder: MailboxObservableQueryBuilder<ConversationMessagesQuery>,
    >(
        &self,
        builder: Builder,
        id: LocalConversationId,
    ) -> Result<Builder::Output, MailboxError> {
        self.user_ctx.sync_conversation_messages(id).await?;

        Ok(builder.build(
            self.user_ctx.tracker_service().clone(),
            ConversationMessagesQuery::new(id),
        ))
    }

    fn starred_label_id(&self) -> MailboxResult<LocalLabelId> {
        self.user_ctx
            .db_read(|conn| conn.resolve_remote_label_id(LabelId::starred()))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::RemoteLabelNotFound(
                LabelId::starred().clone(),
            ))
    }
}
