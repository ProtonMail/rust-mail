use crate::actions::{
    DeleteConversationsAction, LabelConversationsAction, MarkConversationsReadAction,
    MarkConversationsUnreadAction, MoveConversationsAction, UnlabelConversationsAction,
};
use crate::db::{ConversationQuery, LocalConversation, LocalConversationId, LocalLabelId};
use crate::exports::anyhow::anyhow;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_api_mail::proton_api_core::exports::tracing;

impl Mailbox {
    /// Sync the label's messages or conversations.
    ///
    /// Depending on the user's mail settings, this function will either sync the conversations
    /// or the messages of the label.
    ///
    /// # Errors
    /// Returns error if API request or database changes failed.
    pub async fn sync(&self, count: usize) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(self.label_id)? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        let view_mode = label
            .mail_settings_view_mode()
            .unwrap_or(self.user_ctx.with_mail_settings(|s| s.view_mode));
        if let Some(remote_id) = label.rid.clone() {
            tracing::debug!("Syncing {}({})", self.label_id, remote_id);
            let ctx = self.user_ctx.clone();

            let initialized = ctx
                .db_read(|conn| match view_mode {
                    MailSettingsViewMode::Conversations => {
                        conn.check_if_label_is_initialized_conversations(label.id)
                    }
                    MailSettingsViewMode::Messages => {
                        conn.check_if_label_is_initialized_messages(label.id)
                    }
                })
                .map_err(|e| {
                    tracing::error!("Failed to check if label is initialized: {e}");
                    MailContextError::DB(e)
                })?;
            if initialized {
                tracing::debug!("Label {} already initialized, skipping", self.label_id);
                return Ok(());
            }
            tracing::debug!(
                "Label {} not initialized, fetching (mode={:?})",
                self.label_id,
                view_mode
            );

            match view_mode {
                MailSettingsViewMode::Conversations => ctx
                    .sync_first_conversation_page(remote_id, count)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to sync conversations for label: {e}");
                        e
                    }),
                MailSettingsViewMode::Messages => ctx
                    .sync_first_message_page(remote_id, count)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to sync messages for label: {e}");
                        e
                    }),
            }?;

            ctx.db_write(|tx| {
                match view_mode {
                    MailSettingsViewMode::Conversations => {
                        tx.mark_label_as_initialized_conversations(label.id)?;
                    }
                    MailSettingsViewMode::Messages => {
                        tx.mark_label_as_initialized_messages(label.id)?;
                    }
                }
                Ok(())
            })
            .map_err(|e| {
                tracing::error!("Failed to mark label as initialized: {e}");
                MailContextError::DB(e)
            })?;

            Ok(())
        } else {
            Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id))
        }
    }

    pub fn new_conversation_query<Builder: MailboxObservableQueryBuilder<ConversationQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            ConversationQuery::new(self.label_id, limit),
        )
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

    fn starred_label_id(&self) -> MailboxResult<LocalLabelId> {
        self.user_ctx
            .db_read(|conn| conn.resolve_remote_label_id(LabelId::starred()))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::RemoteLabelNotFound(
                LabelId::starred().clone(),
            ))
    }
}
