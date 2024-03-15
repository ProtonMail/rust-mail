use crate::mail::{MailContextError, MailUserContext};
use crate::new_live_query;
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, LiveQueryUpdated, ObservableQuery, SharedLiveQueryBuilder,
};
use proton_mail_common::exports::thiserror;
use proton_mail_common::exports::tracing::error;
use proton_mail_common::proton_api_mail::domain::LabelId;
use proton_mail_common::proton_mail_db::proton_sqlite3::SharedLiveQuery;
use proton_mail_common::proton_mail_db::{ConversationQuery, LocalLabelId};
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

/// A [`Mailbox`] provides a gateway to manipulating messages and conversations for a given label.
#[derive(uniffi::Object)]
pub struct Mailbox {
    mbox: proton_mail_common::Mailbox,
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

new_live_query!(MailboxConversationLiveQuery, ConversationQuery);

const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[uniffi::export]
impl Mailbox {
    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub fn new(ctx: &MailUserContext, label_id: u64) -> Self {
        Self {
            mbox: proton_mail_common::Mailbox::with_id(
                ctx.ctx().clone(),
                LocalLabelId::new(label_id),
            ),
        }
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub fn inbox(ctx: &MailUserContext) -> MailboxResult<Self> {
        Ok(Self {
            mbox: proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), LabelId::inbox())?,
        })
    }

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

/*
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
}*/

struct FFIObservableConversationsQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<ConversationQuery> for FFIObservableConversationsQueryBuilder {
    type Output = Arc<MailboxConversationLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: ConversationQuery) -> Self::Output {
        MailboxConversationLiveQuery::new(tracker, query, self.0)
    }
}
