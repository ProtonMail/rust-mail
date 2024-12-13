use super::MailUserSession;
use crate::errors::{UserSessionError, VoidSessionResult};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[uniffi::export]
impl MailUserSession {
    /// Execute exactly one pending action.
    #[must_use]
    pub async fn execute_pending_action(&self) -> VoidSessionResult {
        self.ctx
            .execute_pending_actions()
            .await
            .map_err(RealProtonMailError::from)
            .map_err(UserSessionError::from)
            .into()
    }

    /// Execute exactly all pending actions.
    pub async fn execute_pending_actions(&self) -> VoidSessionResult {
        self.ctx
            .execute_pending_actions()
            .await
            .map_err(RealProtonMailError::from)
            .map_err(UserSessionError::from)
            .into()
    }
}
