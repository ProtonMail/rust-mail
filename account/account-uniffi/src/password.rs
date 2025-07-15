use proton_account_api::password::PasswordError as RealPasswordError;
use proton_account_api::password::PasswordFlow as RealPasswordFlow;
use proton_account_api::password::state::StateKind;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use uniffi_runtime::async_runtime;
use uniffi_runtime::uniffi_async;

use crate::password_validator::PasswordValidatorService;

/// Errors that can occur during the password change flow, exposed via `UniFFI`.
#[derive(Debug, Error, uniffi::Error)]
pub enum PasswordError {
    #[error("API error: {0}")]
    Api(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid 2FA code")]
    Invalid2FACode,

    #[error("Key unlock error")]
    KeyUnlock,

    #[error("Invalid state")]
    InvalidState,

    #[error("Internal error")]
    Internal,
}

impl From<RealPasswordError> for PasswordError {
    fn from(value: RealPasswordError) -> Self {
        match value {
            RealPasswordError::InvalidState => Self::InvalidState,

            // Auth error caused by invalid 2FA input/token
            RealPasswordError::FlowAuth(_)
            | RealPasswordError::KeySecretSaltFetch(_)
            | RealPasswordError::ServerProof => Self::InvalidCredentials,

            // Key unlock error
            RealPasswordError::MissingPrimaryKey
            | RealPasswordError::KeySecretDecryption
            | RealPasswordError::KeySecretDerivation(_)
            | RealPasswordError::KeyEncoding(_) => Self::KeyUnlock,

            // Api service error
            RealPasswordError::ApiService(e) => e
                .to_proton_error()
                .and_then(|e| e.error)
                .map_or_else(|| Self::Api(e.to_string()), Self::Api),

            // Api error
            RealPasswordError::Api(e) => e
                .body_str()
                .map_or_else(|| Self::Api(e.to_string()), |e| Self::Api(e.to_owned())),

            // Crypto error
            RealPasswordError::Crypto(e) => Self::Crypto(e.to_string()),

            // Store error
            RealPasswordError::Store(_) => Self::Internal,
        }
    }
}

impl From<JoinError> for PasswordError {
    fn from(_: JoinError) -> Self {
        Self::Internal
    }
}

/// Simplified password flow state for UniFFI bindings.
///
/// This enum represents the different states of the password change flow
/// in a simplified form suitable for foreign function interface bindings.
#[derive(uniffi::Enum, Debug)]
pub enum SimplePasswordState {
    /// Waiting for the user's current password.
    WantPass,
    /// Waiting for two-factor authentication code.
    WantTfa,
    /// Waiting for the new password to be set.
    WantChange,
    /// Password change flow completed successfully.
    Complete,
    /// Invalid or error state.
    Invalid,
}

impl From<StateKind> for SimplePasswordState {
    fn from(kind: StateKind) -> Self {
        match kind {
            StateKind::WantPass => Self::WantPass,
            StateKind::WantTfa => Self::WantTfa,
            StateKind::WantChange => Self::WantChange,
            StateKind::Complete => Self::Complete,
            StateKind::Invalid => Self::Invalid,
        }
    }
}

/// Manages the password change process for a user.
#[derive(uniffi::Object)]
pub struct PasswordFlow {
    flow: Arc<Mutex<RealPasswordFlow>>,
}

impl PasswordFlow {
    #[must_use]
    pub fn new(flow: RealPasswordFlow) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
        })
    }
}

#[uniffi_export]
impl PasswordFlow {
    /// Submit the user's current password.
    pub async fn submit_pass(&self, pass: String) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            Ok(flow.lock().await.submit_pass(pass).await?)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit a two-factor authentication code.
    pub async fn submit_totp(&self, code: String) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            Ok(flow.lock().await.submit_totp(code).await?)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Change the account password; leaves the mailbox password (if any) unchanged.
    pub async fn change_pass(
        &self,
        new_pass: String,
    ) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            Ok(flow.lock().await.change_pass(new_pass).await?)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Change the mailbox password; leaves the account password unchanged.
    pub async fn change_mbox_pass(
        &self,
        new_mbox_pass: String,
    ) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            Ok(flow.lock().await.change_mbox_pass(new_mbox_pass).await?)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Returns a password validator service.
    #[must_use]
    pub async fn password_validator(&self) -> Option<Arc<PasswordValidatorService>> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            Ok(Arc::new(PasswordValidatorService::setup(
                flow.lock().await.api()?,
            )))
        })
        .await
        .ok()
    }

    /// Returns whether the account has MBP enabled.
    pub fn has_mbp(&self) -> Result<bool, PasswordError> {
        let mode = async_runtime().block_on(async { self.flow.lock().await.mbp_mode() });

        Ok(mode?.want_mbp())
    }

    /// Get the current state of the PasswordFlow
    #[must_use]
    pub fn get_state(&self) -> SimplePasswordState {
        async_runtime().block_on(async { self.flow.lock().await.kind().unwrap().into() })
    }

    /// Step the flow back to the previous state.
    pub async fn step_back(&self) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async(async move { flow.lock().await.back().map_err(PasswordError::from) }).await?;

        Ok(self.get_state())
    }
}
