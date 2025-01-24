use crate::actions::{new_action_factory, ActionError};
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::Action;
use proton_action_queue::queue::{ActionOutput, Queue, QueuedActionOutput};
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
    ) -> MailContextResult<ActionOutput<T>> {
        self.exclusive.execute_action(action).await
    }

    /// Queue an action for later execution.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be queued.
    pub async fn queue_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<QueuedActionOutput<T>> {
        self.exclusive.queue_action(action).await
    }

    /// Execute exactly one pending action in the queue.
    pub async fn execute_pending_action(&self) -> MailContextResult<()> {
        self.exclusive.execute_pending_action().await
    }

    /// Execute all pending actions in the queue.
    pub async fn execute_pending_actions(&self) -> MailContextResult<usize> {
        self.exclusive.execute_pending_actions().await
    }
}

pub(super) async fn new_action_queue(stash: Stash) -> proton_action_queue::queue::Result<Queue> {
    Queue::with_factory(stash, new_action_factory()).await
}
