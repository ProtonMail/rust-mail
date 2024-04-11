mod conversations;

use crate::mail::{MailSessionError, MailSessionResult, MailUserSession};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::SharedLive;
use proton_mail_common::db::{ConversationQuery, LocalLabelId};
use proton_mail_common::exports::anyhow::anyhow;
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, LiveQueryUpdated, Observable, SharedLiveQueryBuilder,
};
use proton_mail_common::exports::thiserror;
use proton_mail_common::exports::tracing::error;
use proton_mail_common::proton_api_mail::domain::LabelId;
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
        MailSessionError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_mail_common::exports::proton_action_queue::QueueError),
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
    pub async fn new(ctx: &MailUserSession, label_id: u64) -> MailSessionResult<Self> {
        let mbox =
            proton_mail_common::Mailbox::with_id(ctx.ctx().clone(), LocalLabelId::new(label_id));
        let join_handler = mbox
            .user_context()
            .mail_context()
            .clone()
            .async_runtime()
            .spawn(async move {
                if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT, None).await {
                    error!("Could not sync mailbox: {e}");
                }
                mbox
            })
            .await;
        match join_handler {
            Ok(mbox) => Ok(Self { mbox }),
            Err(err) => Err(MailSessionError::Other(anyhow!(
                "Failed to join task: {err}"
            ))),
        }
    }

    /// Create a new mailbox for a given remote id.
    #[uniffi::constructor]
    pub fn with_remote_id(ctx: &MailUserSession, label_id: LabelId) -> MailboxResult<Self> {
        Ok(Self {
            mbox: proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), &label_id)?,
        })
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub fn inbox(ctx: &MailUserSession) -> MailboxResult<Self> {
        Ok(Self {
            mbox: proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), LabelId::inbox())?,
        })
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
            proton_mail_common::MailboxError::ActionQueue(e) => Self::ActionQueue(e),
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
