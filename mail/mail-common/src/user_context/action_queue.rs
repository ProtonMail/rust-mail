use crate::actions::MailActionError;
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::Action;
use proton_action_queue::queue::QueuedActionOutput;

impl MailUserContext {
    /// Queue an action for later execution.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be queued.
    pub async fn queue_action<T: Action<Error = MailActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<QueuedActionOutput<T>> {
        Ok(self.user_context.queue().queue_action(action).await?)
    }
}
