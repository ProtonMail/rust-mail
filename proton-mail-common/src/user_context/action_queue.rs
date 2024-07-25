use crate::actions::{new_action_factory, ActionError};
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::{Action, Id as ActionId};
use proton_action_queue::queue::{ActionStatus, Queue};
use stash::stash::Stash;

impl MailUserContext {
    /// Execute an action immediately.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be executed.
    pub async fn execute_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<ActionStatus<T::Output>> {
        Ok(self
            .action_queue
            .apply_action(self.session(), action)
            .await?)
    }
    /// Queue an action for later execution.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be queued.
    pub async fn queue_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<ActionId> {
        Ok(self.action_queue.queue_action(action).await?)
    }

    /// Execute exactly one pending action in the queue.
    pub async fn execute_pending_action(&self) -> MailContextResult<()> {
        Ok(self
            .action_queue
            .execute_with_limit(self.session(), 1)
            .await?)
    }

    /// Execute all pending actions in the queue.
    pub async fn execute_pending_actions(&self) -> MailContextResult<()> {
        Ok(self.action_queue.execute_all(self.session()).await?)
    }
}

pub(super) async fn new_action_queue(stash: Stash) -> proton_action_queue::queue::Result<Queue> {
    Queue::with_factory(stash, new_action_factory()).await
}
