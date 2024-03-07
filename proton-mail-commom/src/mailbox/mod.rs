mod conversation;
mod labels;

use crate::{MailContextError, MailUserContext, MailUserContextInitializationCallback};
use proton_api_mail::domain::LabelId;
use proton_api_mail::proton_api_core::exports::thiserror;
use proton_api_mail::proton_api_core::exports::tracing::error;
use proton_mail_db::proton_sqlite3::{
    InProcessTrackerService, LiveQuery, LiveQueryBuilder, ObservableQuery,
};
use proton_mail_db::{LocalLabel, LocalLabelId};

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
}

/// Abstraction trait to make it easier to integrate mailbox in different target platforms. E.g.:
/// Some platforms are able to use the [`LiveQuery`] type and other platform may benefit from
/// a different solution.
pub trait MailboxObservableQueryBuilder: Send + Sync {
    type Type<Q: ObservableQuery>;

    fn build<Q: ObservableQuery>(
        &self,
        tracker: InProcessTrackerService,
        query: Q,
    ) -> Self::Type<Q>;
}

/// Implementation of [`MailboxObservableQueryBuilder`] which returns live query objects.
pub struct MailboxLiveQueryBuilder {}

impl MailboxObservableQueryBuilder for MailboxLiveQueryBuilder {
    type Type<Q: ObservableQuery> = LiveQuery<Q>;

    fn build<Q: ObservableQuery>(
        &self,
        tracker: InProcessTrackerService,
        query: Q,
    ) -> Self::Type<Q> {
        LiveQueryBuilder::new(tracker)
            .with_background_initializer()
            .build(query)
    }
}

pub type MailboxResult<T> = Result<T, MailboxError>;

pub struct Mailbox<Builder: MailboxObservableQueryBuilder> {
    user_ctx: MailUserContext,
    active_label: LocalLabel,
    builder: Builder,
}

pub trait MailboxBackgroundResult<T: Send>: Send + Sync {
    fn on_background_error(&self, result: MailboxResult<T>);
}

impl<T: Send, F: Fn(MailboxResult<T>) + Send + Sync> MailboxBackgroundResult<T> for F {
    fn on_background_error(&self, result: MailboxResult<T>) {
        (self)(result);
    }
}

impl<Builder: MailboxObservableQueryBuilder> Mailbox<Builder> {
    pub fn new(user_ctx: MailUserContext, builder: Builder) -> MailboxResult<Self> {
        let inbox_id = LabelId::inbox();
        let Some(label) = user_ctx.get_label_with_remote_id(&inbox_id)? else {
            return Err(MailboxError::RemoteLabelNotFound(inbox_id));
        };

        Ok(Self {
            user_ctx,
            builder,
            active_label: label,
        })
    }

    pub fn user_context(&self) -> &MailUserContext {
        &self.user_ctx
    }
    pub fn active_label(&self) -> &LocalLabel {
        &self.active_label
    }

    pub fn refresh(&self, cb: Box<dyn MailUserContextInitializationCallback>) -> MailboxResult<()> {
        let ctx = self.user_ctx.clone();

        let Some(rid) = self.active_label.rid.clone() else {
            return Err(MailboxError::LabelDoesNotHaveRemoteId(self.active_label.id));
        };

        ctx.initialize(rid, cb);
        Ok(())
    }

    pub fn logout(&self, cb: Box<dyn MailboxBackgroundResult<()>>) {
        let ctx = self.user_ctx.clone();
        self.user_ctx
            .mail_context()
            .async_runtime()
            .spawn(async move {
                let result = ctx.logout().await;
                cb.on_background_error(result.map_err(|e| e.into()));
            });
    }
}
