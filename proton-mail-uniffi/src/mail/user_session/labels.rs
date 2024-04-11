use crate::mail::mailbox::MailboxLiveQueryUpdatedCallback;
use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::db::{LabelsByTypeQueryWithConversationCount, LocalLabel};
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, Observable, SharedLive, SharedLiveQueryBuilder,
};
use proton_mail_common::proton_api_mail::domain::LabelType;
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

    /// Return the list of labels of type Folder into which a conversations or
    /// message can be moved.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub fn movable_folders(&self) -> Result<Vec<LocalLabel>, MailSessionError> {
        Ok(self.ctx.get_labels_by_type(LabelType::Label)?)
    }

    /// Return the list of labels of type Label that can be applied to conversations or
    /// messages.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub fn applicable_labels(&self) -> Result<Vec<LocalLabel>, MailSessionError> {
        Ok(self.ctx.get_labels_by_type(LabelType::Label)?)
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
