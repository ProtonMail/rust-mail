use super::MailUserSession;
use crate::errors::VoidSessionResult;

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    #[must_use]
    pub fn execute_pending_action(&self) -> VoidSessionResult {
        drop(self.ctx.execute_pending_action());
        VoidSessionResult::Ok
    }

    /// Execute exactly all pending actions.
    #[must_use]
    pub fn execute_pending_actions(&self) -> VoidSessionResult {
        drop(self.ctx.execute_pending_actions());
        VoidSessionResult::Ok
    }
}
