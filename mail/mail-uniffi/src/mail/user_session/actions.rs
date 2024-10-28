use super::MailUserSession;
use crate::errors::user_session::VoidUserSessionResult;

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    #[must_use]
    pub fn execute_pending_action(&self) -> VoidUserSessionResult {
        drop(self.ctx.execute_pending_action());
        VoidUserSessionResult::Ok
    }

    /// Execute exactly all pending actions.
    #[must_use]
    pub fn execute_pending_actions(&self) -> VoidUserSessionResult {
        drop(self.ctx.execute_pending_actions());
        VoidUserSessionResult::Ok
    }
}
