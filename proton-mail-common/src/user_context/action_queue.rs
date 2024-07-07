use crate::actions::new_action_factory;
use crate::{MailContextResult, MailUserContext};
use anyhow::anyhow;
use proton_action_queue::{Action, ActionQueue, SessionProviderError};
use proton_api_core::session::Session;
use stash::stash::Stash;
use std::sync::Weak;
use tracing::error;

impl MailUserContext {
    /// Queue an action for later execution.
    pub async fn queue_action<T: Action>(&self, action: T) -> MailContextResult<()> {
        self.action_queue.queue_action(&action).await?;
        Ok(())
    }

    /// Execute exactly one pending action in the queue.
    pub async fn execute_pending_action(&self) -> MailContextResult<()> {
        self.action_queue.consume_pending_with_limit(1).await?;
        Ok(())
    }

    /// Execute all pending actions in the queue.
    pub async fn execute_pending_actions(&self) -> MailContextResult<()> {
        self.action_queue.consume_pending().await?;
        Ok(())
    }
}

struct SessionProvider(Weak<MailUserContext>);

impl proton_action_queue::SessionProvider for SessionProvider {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError> {
        let Some(ctx) = self.0.upgrade() else {
            error!("Could not upgrade context, does it still exist");
            return Err(SessionProviderError::Other(anyhow!(
                "Could not upgrade context"
            )));
        };

        Ok(ctx.user_context.session().clone())
    }
}

pub(super) fn new_action_queue(
    mail_user_context: Weak<MailUserContext>,
    stash: Stash,
) -> ActionQueue {
    ActionQueue::new(
        stash,
        Box::new(SessionProvider(mail_user_context.clone())),
        new_action_factory(mail_user_context),
    )
}
