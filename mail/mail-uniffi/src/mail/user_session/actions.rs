use super::MailUserSession;
use crate::errors::{unexpected::UnexpectedError, ProtonError, UserSessionError};
use futures::TryFutureExt;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[uniffi_export]
impl MailUserSession {
    /// Execute exactly one pending action.
    #[must_use]
    pub async fn execute_pending_action(&self) -> Result<(), UserSessionError> {
        self.ctx()?
            .execute_pending_action()
            .map_err(RealProtonMailError::from)
            .map_err(UserSessionError::from)
            .await
    }

    /// Execute exactly all pending actions.
    pub async fn execute_pending_actions(&self) -> Result<u64, UserSessionError> {
        let n = self
            .ctx()?
            .execute_pending_actions()
            .map_err(RealProtonMailError::from)
            .map_err(UserSessionError::from)
            .await?;

        Ok(n.try_into()
            .or(Err(UnexpectedError::Internal))
            .map_err(ProtonError::Unexpected)?)
    }
}
