use crate::mail::mailbox::{Observable, SharedLive, SharedLiveQueryBuilder};
use crate::mail::Mailbox;
use crate::mail::{MailboxError, MailboxLiveQueryUpdatedCallback};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::InProcessTrackerService;
use proton_mail_common::db::LabelCountsQuery;
use proton_mail_common::MailboxObservableQueryBuilder;
use std::sync::Arc;

#[uniffi::export]
impl Mailbox {
    /// Create a new live query which track the total number and unread number of items present
    /// in the current mailbox.
    ///
    /// The returned values will track either messages or conversations depending on the
    /// current mailbox's view mode.
    ///
    /// # Errors
    /// Return error if the database operation failed.
    pub fn new_item_live_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Result<Arc<MailboxItemCountLiveQuery>, MailboxError> {
        let builder = FFIMailboxItemCountQueryBuilder(cb);
        Ok(self.mbox.new_label_item_count_query(builder)?)
    }
}

new_live_query!(MailboxItemCountLiveQuery, LabelCountsQuery);

struct FFIMailboxItemCountQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<LabelCountsQuery> for FFIMailboxItemCountQueryBuilder {
    type Output = Arc<MailboxItemCountLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: LabelCountsQuery) -> Self::Output {
        MailboxItemCountLiveQuery::new(tracker, query, self.0)
    }
}
