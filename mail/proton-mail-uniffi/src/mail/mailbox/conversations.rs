use crate::mail::mailbox::{FFIObservableConversationsQueryBuilder, DEFAULT_CONVERSATION_COUNT};
use crate::mail::{
    Mailbox, MailboxConversationLiveQuery, MailboxError, MailboxLiveQueryUpdatedCallback,
};
use proton_mail_common::exports::tracing::error;
use proton_mail_common::proton_mail_db::LocalConversationId;
use std::sync::Arc;

#[uniffi::export]
impl Mailbox {
    /// Create a live query for conversations for the currently selected label. If you
    /// change the mailbox label with `switch_label` you need to create a new instance.
    pub fn new_conversation_live_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxConversationLiveQuery> {
        //TODO: Improve this.
        let limit = usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT);
        if let Err(e) = self.mbox.sync(limit, None) {
            error!("Could not sync mailbox: {e}");
        }
        let builder = FFIObservableConversationsQueryBuilder(cb);
        self.mbox.new_conversation_query(builder, limit)
    }

    /// Delete/Destroy the given conversations for the current mailbox.
    pub fn delete_conversations(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .delete_conversations(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Mark the given conversations as read.
    pub fn mark_conversations_read(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .mark_conversations_read(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Mark the given conversations as unread.
    pub fn mark_conversations_unread(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .mark_conversations_unread(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }
}
