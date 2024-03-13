use crate::mail::{MailContextError, MailUserContext};
use proton_mail_common::exports::parking_lot::RwLock;
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, LiveQueryUpdated, ObservableQuery, SharedLiveQueryBuilder,
};
use proton_mail_common::exports::thiserror;
use proton_mail_common::proton_api_mail::domain::{LabelId, LabelType};
use proton_mail_common::proton_mail_db::proton_sqlite3::{SharedLiveQuery, SharedLiveQueryUpdated};
use proton_mail_common::proton_mail_db::{
    ConversationQuery, LabelsByTypeQueryWithConversationCount, LocalConversation, LocalLabel,
    LocalLabelId, LocalLabelWithCount,
};
use proton_mail_common::MailboxObservableQueryBuilder;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(LocalLabelId),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(LocalLabelId),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailContextError,
    ),
}

pub type MailboxResult<T> = Result<T, MailboxError>;

#[derive(uniffi::Object)]
pub struct Mailbox {
    mbox: RwLock<proton_mail_common::Mailbox>,
}

/// Callback for operations that get scheduled in the background and return no result.
#[uniffi::export(callback_interface)]
pub trait MailboxBackgroundResult: Send + Sync {
    fn on_background_result(&self, error: Option<MailboxError>);
}

/// Callback for a labels view data change.
#[uniffi::export(callback_interface)]
pub trait MailboxLiveQueryUpdatedCallback: Send + Sync {
    fn on_updated(&self);
}

impl LiveQueryUpdated for Box<dyn MailboxLiveQueryUpdatedCallback> {
    fn on_live_query_updated(&self) {
        self.on_updated()
    }
}

impl SharedLiveQueryUpdated for Box<dyn MailboxLiveQueryUpdatedCallback> {}

macro_rules! new_live_query {
    ($name:ident, $query:ident) => {
        /// Observable query.
        #[derive(uniffi::Object)]
        pub struct $name(SharedLiveQuery<$query>);

        #[uniffi::export]
        impl $name {
            /// Get the latest value for this Query.
            pub fn value(&self) -> <$query as ObservableQuery>::Output {
                self.0.value().clone()
            }

            /// Terminate the observer for this query and stop receiving updates.
            pub fn disconnect(&self) {
                self.0.disconnect();
            }
        }

        impl $name {
            fn new(
                tracker: InProcessTrackerService,
                query: $query,
                cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
            ) -> Arc<Self> {
                Arc::new(Self(
                    SharedLiveQueryBuilder::new(tracker)
                        .with_background_initializer()
                        .with_callback(cb)
                        .build(query),
                ))
            }
        }
    };
}

new_live_query!(MailboxConversationLiveQuery, ConversationQuery);
new_live_query!(
    MailboxLabelsLiveQuery,
    LabelsByTypeQueryWithConversationCount
);

const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[uniffi::export]
impl Mailbox {
    #[uniffi::constructor]
    pub fn new(ctx: &MailUserContext) -> MailboxResult<Self> {
        Ok(Self {
            mbox: RwLock::new(proton_mail_common::Mailbox::new(ctx.ctx().clone())?),
        })
    }

    /// Return the currently selected label.
    pub fn active_label(&self) -> LocalLabel {
        self.mbox.read().active_label().clone()
    }

    /// Switch the mailbox to another label.
    pub fn switch_label(
        &self,
        label_id: u64,
        message_count: i64,
        cb: Option<Box<dyn MailboxBackgroundResult>>,
    ) -> MailboxResult<()> {
        let mut guard = self.mbox.write();

        let cb = cb.map(|cb| FFIMailboxBackgroundVoidResult::from(cb).boxed());

        guard.switch_label(
            LocalLabelId::from(label_id),
            usize::try_from(message_count).unwrap_or(DEFAULT_CONVERSATION_COUNT),
            cb,
        )?;
        Ok(())
    }

    /// Create a query observer for conversations for the currently selected label. If you
    /// change the mailbox label with `switch_label` you need to create a new instance.
    pub fn new_conversation_observed_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxConversationLiveQuery> {
        let builder = FFIObservableConversationsQueryBuilder(cb);
        self.mbox.read().new_conversation_query(
            builder,
            usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT),
        )
    }

    /// Get conversations for the current selected label.
    pub fn conversations(&self, count: i64) -> MailboxResult<Vec<LocalConversation>> {
        let v = self
            .mbox
            .read()
            .conversations(usize::try_from(count).unwrap_or(DEFAULT_CONVERSATION_COUNT))?;
        Ok(v)
    }

    /// Get list of ordered labels by type.
    pub fn labels_by_type(&self, label_type: LabelType) -> MailboxResult<Vec<LocalLabelWithCount>> {
        let v = self.mbox.read().get_labels_by_type(label_type)?;
        Ok(v)
    }

    /// Create a query observer on labels of type System.
    pub fn new_system_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.mbox.read().new_system_labels_live_query(builder)
    }

    /// Create a query observer on labels of type Folder.
    pub fn new_folder_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.mbox.read().new_folder_labels_live_query(builder)
    }

    /// Create a query observer on labels of type Label.
    pub fn new_label_labels_observed_query(
        &self,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxLabelsLiveQuery> {
        let builder = FFIObservableLabelsQueryBuilder(cb);
        self.mbox.read().new_label_labels_live_query(builder)
    }
}

impl From<proton_mail_common::MailboxError> for MailboxError {
    fn from(value: proton_mail_common::MailboxError) -> Self {
        match value {
            proton_mail_common::MailboxError::LabelNotFound(e) => Self::LabelNotFound(e),
            proton_mail_common::MailboxError::RemoteLabelNotFound(e) => {
                Self::RemoteLabelNotFound(e)
            }
            proton_mail_common::MailboxError::LabelDoesNotHaveRemoteId(e) => {
                Self::LabelDoesNotHaveRemoteId(e)
            }
            proton_mail_common::MailboxError::Context(e) => Self::Context(e.into()),
        }
    }
}

struct FFIMailboxBackgroundVoidResult(Box<dyn MailboxBackgroundResult>);

impl FFIMailboxBackgroundVoidResult {
    fn boxed(self) -> Box<dyn proton_mail_common::MailboxBackgroundResult<()>> {
        Box::new(self)
    }
}

impl From<Box<dyn MailboxBackgroundResult>> for FFIMailboxBackgroundVoidResult {
    fn from(value: Box<dyn MailboxBackgroundResult>) -> Self {
        Self(value)
    }
}

impl proton_mail_common::MailboxBackgroundResult<()> for FFIMailboxBackgroundVoidResult {
    fn on_background_result(&self, result: proton_mail_common::MailboxResult<()>) {
        let result = if let Err(e) = result {
            Some(e.into())
        } else {
            None
        };
        self.0.on_background_result(result);
    }
}

struct FFIObservableConversationsQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<ConversationQuery> for FFIObservableConversationsQueryBuilder {
    type Output = Arc<MailboxConversationLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: ConversationQuery) -> Self::Output {
        MailboxConversationLiveQuery::new(tracker, query, self.0)
    }
}

struct FFIObservableLabelsQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<LabelsByTypeQueryWithConversationCount>
    for FFIObservableLabelsQueryBuilder
{
    type Output = Arc<MailboxLabelsLiveQuery>;

    fn build(
        self,
        tracker: InProcessTrackerService,
        query: LabelsByTypeQueryWithConversationCount,
    ) -> Self::Output {
        MailboxLabelsLiveQuery::new(tracker, query, self.0)
    }
}
