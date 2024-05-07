use crate::mail::mailbox::DEFAULT_CONVERSATION_COUNT;
use crate::mail::mailbox::{Observable, SharedLive, SharedLiveQueryBuilder};
use crate::mail::{Mailbox, MailboxError, MailboxLiveQueryUpdatedCallback};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::InProcessTrackerService;
use proton_mail_common::db::MessageQuery;
use proton_mail_common::MailboxObservableQueryBuilder;
use std::sync::Arc;

#[uniffi::export]
impl Mailbox {
    /// Create a live query for messages for the currently selected label.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Messages`].
    pub fn new_message_live_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Result<Arc<MailboxMessageLiveQuery>, MailboxError> {
        let limit = usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT);
        let builder = FFIObservableMessagesQueryBuilder(cb);
        Ok(self.mbox.new_messages_query(builder, limit)?)
    }
}

new_live_query!(MailboxMessageLiveQuery, MessageQuery);

struct FFIObservableMessagesQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<MessageQuery> for FFIObservableMessagesQueryBuilder {
    type Output = Arc<MailboxMessageLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: MessageQuery) -> Self::Output {
        MailboxMessageLiveQuery::new(tracker, query, self.0)
    }
}
