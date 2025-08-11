use crate::actions::MailActionError;
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::Action;
use proton_action_queue::queue::QueuedActionOutput;

impl MailUserContext {
    pub async fn queue_action<T>(&self, action: T) -> MailContextResult<QueuedActionOutput<T>>
    where
        T: Action<Error = MailActionError>,
    {
        Ok(self.user_context.queue().queue_action(action).await?)
    }
}
