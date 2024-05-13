use super::{MailSessionError, MailUserSession};

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    pub fn execute_pending_action(&self) -> Result<(), MailSessionError> {
        drop(self.ctx.execute_pending_action());
        Ok(())
    }

    /// Execute exactly all pending actions.
    pub fn execute_pending_actions(&self) -> Result<(), MailSessionError> {
        drop(self.ctx.execute_pending_actions());
        Ok(())
    }
}
