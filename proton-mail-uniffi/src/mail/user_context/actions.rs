use super::{MailContextError, MailUserContext};
use crate::mail::map_task_join_error;

#[uniffi::export]
impl MailUserContext {
    /// Execute exactly one pending action.
    pub async fn execute_pending_action(&self) -> Result<(), MailContextError> {
        let ctx = self.ctx.clone();
        self.ctx
            .mail_context()
            .async_runtime()
            .spawn_blocking(move || -> Result<(), MailContextError> {
                ctx.execute_pending_action()?;
                Ok(())
            })
            .await
            .map_err(map_task_join_error)?
    }

    /// Execute exactly all pending actions.
    pub async fn execute_pending_actions(&self) -> Result<(), MailContextError> {
        let ctx = self.ctx.clone();
        self.ctx
            .mail_context()
            .async_runtime()
            .spawn_blocking(move || -> Result<(), MailContextError> {
                ctx.execute_pending_actions()?;
                Ok(())
            })
            .await
            .map_err(map_task_join_error)?
    }
}
