mod conversation;
use crate::db::proton_sqlite3::{InProcessTrackerService, ObservableQuery};
use crate::db::LocalLabelId;
use crate::{MailContextError, MailUserContext, MailUserContextInitializationCallback};
use proton_api_mail::domain::LabelId;
use proton_api_mail::proton_api_core::exports::thiserror;
use proton_api_mail::proton_api_core::exports::tracing::error;

#[derive(Debug, thiserror::Error)]
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
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::QueueError),
}

/// Abstraction trait to make it easier to integrate mail in different target platforms. E.g.:
/// Some platforms are able to use the [`crate::db::proton_sqlite3::LiveQuery`] type and other
/// platform may benefit from a different solution.
pub trait MailboxObservableQueryBuilder<Q: ObservableQuery> {
    type Output;

    fn build(self, tracker: InProcessTrackerService, query: Q) -> Self::Output;
}

impl<Q: ObservableQuery, R, F: FnOnce(InProcessTrackerService, Q) -> R>
    MailboxObservableQueryBuilder<Q> for F
{
    type Output = R;

    fn build(self, tracker: InProcessTrackerService, query: Q) -> Self::Output {
        (self)(tracker, query)
    }
}

pub type MailboxResult<T> = Result<T, MailboxError>;

pub struct Mailbox {
    user_ctx: MailUserContext,
    label_id: LocalLabelId,
}

pub trait MailboxBackgroundResult<T: Send>: Send + Sync {
    fn on_background_result(&self, result: MailboxResult<T>);
}

impl<T: Send, F: Fn(MailboxResult<T>) + Send + Sync> MailboxBackgroundResult<T> for F {
    fn on_background_result(&self, result: MailboxResult<T>) {
        (self)(result);
    }
}

impl Mailbox {
    pub fn with_remote_id(user_ctx: MailUserContext, label_id: &LabelId) -> MailboxResult<Self> {
        let Some(label) = user_ctx.get_label_with_remote_id(label_id)? else {
            return Err(MailboxError::RemoteLabelNotFound(label_id.clone()));
        };

        Ok(Self {
            user_ctx,
            label_id: label.id,
        })
    }

    pub fn with_id(user_ctx: MailUserContext, label_id: LocalLabelId) -> Self {
        Self { user_ctx, label_id }
    }

    pub fn user_context(&self) -> &MailUserContext {
        &self.user_ctx
    }
    pub fn label_id(&self) -> LocalLabelId {
        self.label_id
    }

    pub fn refresh(&self, cb: Box<dyn MailUserContextInitializationCallback>) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(self.label_id)? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        let Some(rid) = label.rid else {
            return Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id));
        };

        self.user_ctx.initialize(rid, cb);
        Ok(())
    }
}
