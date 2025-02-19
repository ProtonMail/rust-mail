use crate::actions::{new_action_factory, ActionError};
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::Action;
use proton_action_queue::queue::{Queue, QueuedActionOutput};
use stash::stash::Stash;

impl MailUserContext {
    /// Queue an action for later execution.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be queued.
    pub async fn queue_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<QueuedActionOutput<T>> {
        Ok(self.action_queue.queue_action(action).await?)
    }

    /// Execute exactly one pending action in the queue.
    pub async fn execute_pending_action(&self) -> MailContextResult<()> {
        self.default_queue_executor.execute_one().await?;
        Ok(())
    }

    /// Execute all pending actions in the queue.
    pub async fn execute_pending_actions(&self) -> MailContextResult<usize> {
        Ok(self.default_queue_executor.execute_all().await?)
    }
}

pub(super) async fn new_action_queue(stash: Stash) -> proton_action_queue::queue::Result<Queue> {
    Queue::with_factory(stash, new_action_factory()).await
}
