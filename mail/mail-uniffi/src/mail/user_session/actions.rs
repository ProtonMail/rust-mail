use super::{MailSessionError, MailUserSession};

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    pub async fn execute_pending_action(&self) -> Result<(), MailSessionError> {
        Ok(self.ctx.execute_pending_actions().await?)
    }

    /// Execute exactly all pending actions.
    pub async fn execute_pending_actions(&self) -> Result<(), MailSessionError> {
        Ok(self.ctx.execute_pending_actions().await?)
    }
}
