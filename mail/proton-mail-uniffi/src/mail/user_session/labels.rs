use crate::mail::mailbox::MailboxLiveQueryUpdatedCallback;
use crate::mail::MailUserSession;
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, ObservableQuery, SharedLiveQuery, SharedLiveQueryBuilder,
};
use proton_mail_common::proton_mail_db::LabelsByTypeQueryWithConversationCount;
use proton_mail_common::MailboxObservableQueryBuilder;
use std::sync::Arc;

#[uniffi::export]
impl MailUserSession {
    /// Create a query observer on labels of type System.
    pub fn new_system_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.ctx.new_system_labels_live_query(builder)
    }

    /// Create a query observer on labels of type Folder.
    pub fn new_folder_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.ctx.new_folder_labels_live_query(builder)
    }

    /// Create a query observer on labels of type Label.
    pub fn new_label_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.ctx.new_label_labels_live_query(builder)
    }
}

crate::new_live_query!(MailLabelsLiveQuery, LabelsByTypeQueryWithConversationCount);

struct FFIObservableLabelsQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>
    for FFIObservableLabelsQueryBuilder
{
    type Output = Arc<MailLabelsLiveQuery>;

    fn build(
        self,
        tracker: InProcessTrackerService,
        query: LabelsByTypeQueryWithConversationCount,
    ) -> Self::Output {
        MailLabelsLiveQuery::new(tracker, query, self.0)
    }
}
