use super::MailUserSession;
use crate::errors::VoidProtonMailResult;

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    #[must_use]
    pub fn execute_pending_action(&self) -> VoidProtonMailResult {
        drop(self.ctx.execute_pending_action());
        VoidProtonMailResult::Ok
    }

    /// Execute exactly all pending actions.
    #[must_use]
    pub fn execute_pending_actions(&self) -> VoidProtonMailResult {
        drop(self.ctx.execute_pending_actions());
        VoidProtonMailResult::Ok
    }
}
