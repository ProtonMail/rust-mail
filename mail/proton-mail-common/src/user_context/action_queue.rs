use crate::actions::new_action_factory;
use crate::exports::anyhow::anyhow;
use crate::exports::tracing::error;
use crate::{MailContextResult, MailUserContext, WeakMailUserContext};
use proton_action_queue::{Action, ActionQueue, SessionProviderError};
use proton_api_mail::proton_api_core::Session;

impl MailUserContext {
    /// Queue an action for later execution.
    pub async fn queue_action<T: Action>(&self, action: T) -> MailContextResult<()> {
        self.inner.action_queue.queue_action(&action).await?;
        Ok(())
    }

    /// Execute exactly one pending action in the queue.
    pub async fn execute_pending_action(&self) -> MailContextResult<()> {
        self.inner.action_queue.consume_pending_with_limit(1).await?;
        Ok(())
    }

    /// Execute all pending actions in the queue.
    pub async fn execute_pending_actions(&self) -> MailContextResult<()> {
        self.inner.action_queue.consume_pending().await?;
        Ok(())
    }
}

struct SessionProvider(WeakMailUserContext);

impl proton_action_queue::SessionProvider for SessionProvider {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError> {
        let Some(ctx) = self.0.upgrade() else {
            error!("Could not upgrade context, does it still exist");
            return Err(SessionProviderError::Other(anyhow!(
                "Could not upgrade context"
            )));
        };

        Ok(ctx.inner.user_context.session().clone())
    }
}

pub(super) fn new_action_queue(mail_user_context: WeakMailUserContext) -> ActionQueue {
    ActionQueue::new(
        mail_user_context.inner.upgrade().unwrap().user_context.stash().clone(),
        Box::new(SessionProvider(mail_user_context.clone())),
        new_action_factory(mail_user_context),
    )
}
