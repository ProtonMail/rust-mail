use crate::{
    Mailbox, MailboxBackgroundResult, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_mail_db::{ConversationQuery, LocalConversationWithContext, LocalLabelId};
impl Mailbox {
    pub fn switch_label(
        &mut self,
        label_id: LocalLabelId,
        conversation_count: usize,
        cb: Option<Box<dyn MailboxBackgroundResult<()>>>,
    ) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(label_id)? else {
            return Err(MailboxError::LabelNotFound(label_id));
        };

        self.active_label = label;
        if let Some(remote_id) = self.active_label.rid.clone() {
            tracing::debug!("Selecting label {}({})", self.active_label.id, remote_id);
            let ctx = self.user_ctx.clone();
            self.user_ctx
                .mail_context()
                .async_runtime()
                .spawn(async move {
                    //TODO: check db if we actually need to sync messages.
                    let r = ctx
                        .sync_first_conversation_page(remote_id, conversation_count)
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to sync conversations for labels: {e}");
                            e.into()
                        });
                    if let Some(cb) = cb {
                        cb.on_background_result(r)
                    }
                });
        } else {
            tracing::warn!("Local label {} has no remote id", self.active_label.id);
        }
        Ok(())
    }

    pub fn new_conversation_query<Builder: MailboxObservableQueryBuilder<ConversationQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            ConversationQuery::new(self.active_label.id, limit),
        )
    }

    pub fn conversations(&self, count: usize) -> MailboxResult<Vec<LocalConversationWithContext>> {
        let v = self
            .user_ctx
            .conversations_with_context_for_label(self.active_label.id, count)?;
        Ok(v)
    }
}
