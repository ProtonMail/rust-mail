use crate::actions::{
    DeleteConversationsAction, LabelConversationsAction, MarkConversationsReadAction,
    MarkConversationsUnreadAction, MoveConversationsAction, UnlabelConversationsAction,
};
use crate::db::{
    ConversationQuery, DBResult, LocalConversation, LocalConversationId, LocalLabelId,
};
use crate::{Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult};
use proton_api_mail::proton_api_core::exports::tracing;

impl Mailbox {
    pub async fn sync(&self, conversation_count: usize) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(self.label_id)? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        if let Some(remote_id) = label.rid.clone() {
            tracing::debug!("Syncing {}({})", self.label_id, remote_id);
            let ctx = self.user_ctx.clone();

            if label.initialized {
                tracing::debug!("Label {} already initialized, skipping", self.label_id);
                return Ok(());
            }
            tracing::debug!("Label {} not initialized, fetching", self.label_id);

            let result = ctx
                .sync_first_conversation_page(remote_id, conversation_count)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to sync conversations for labels: {e}");
                    e.into()
                });

            let connection = ctx.new_db_connection();
            match connection {
                Ok(mut connection) => {
                    let result = connection.tx(|tx| -> DBResult<()> {
                        tx.mark_label_as_initialized(label.id)?;
                        Ok(())
                    });
                    if let Err(e) = result {
                        tracing::error!("Failed to mark label as initialized: {e}");
                    }
                }
                Err(e) => tracing::error!("Failed to get db connection: {e}"),
            }
            result
        } else {
            tracing::warn!("Local label {} has no remote id", self.label_id);
            Ok(())
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
}
