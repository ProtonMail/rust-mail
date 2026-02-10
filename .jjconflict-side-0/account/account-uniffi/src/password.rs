use muon::common::IntoDyn;
use proton_account_api::password::state::StateKind;
use proton_account_api::password::{FlowAuthError, PasswordError as RealPasswordError};
use proton_account_api::password::{LoginFailedReason, PasswordFlow as RealPasswordFlow};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use tracing::warn;
use uniffi_runtime::async_runtime;
use uniffi_runtime::uniffi_async;

use crate::login::datatypes::{Fido2RequestFfi, Fido2ResponseFfi};
use crate::password_validator::PasswordType;
use crate::password_validator::PasswordValidatorService;
use crate::password_validator::PasswordValidatorServiceToken;

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

    #[error("TOTP code was already used")]
    Reused2FACode,

    #[error("Invalid recovery code")]
    InvalidRecoveryCode,

    #[error("Key unlock error")]
    KeyUnlock,

    #[error("Invalid state")]
    InvalidState,

    #[error("Internal error")]
    Internal,

    #[error("Password empty")]
    PasswordEmpty,

    #[error("Passwords do not match")]
    PasswordsNotMatching,

    #[error("Password not validated")]
    PasswordNotValidated,

    #[error("Password validation mismatch")]
    PasswordValidationMismatch,
}

impl From<RealPasswordError> for PasswordError {
    fn from(value: RealPasswordError) -> Self {
        match value {
            RealPasswordError::InvalidState => Self::InvalidState,

            // Auth error caused by invalid 2FA input/token
            RealPasswordError::FlowAuth(flow_auth_error) => match flow_auth_error {
                FlowAuthError::PasswordWrong(details) => match details.login_failed_reason {
                    LoginFailedReason::TotpWrong => Self::Invalid2FACode,
                    LoginFailedReason::TotpReuse => Self::Reused2FACode,
                    LoginFailedReason::RecoveryPhrase => Self::InvalidRecoveryCode,
                    LoginFailedReason::Other => Self::InvalidCredentials,
                },
                FlowAuthError::Other(err) => {
                    warn!(?err);
                    Self::InvalidCredentials
                }
            },
            RealPasswordError::KeySecretSaltFetch(_) | RealPasswordError::ServerProof => {
                Self::InvalidCredentials
            }

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

/// Simplified password flow state for FFI bindings.
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
    /// Get the current state of the `PasswordFlow`
    #[must_use]
    pub fn get_state(&self) -> SimplePasswordState {
        async_runtime().block_on(async { self.flow.lock().await.kind().unwrap().into() })
    }

    /// Submit a two-factor authentication code.
    pub async fn submit_totp(&self, code: String) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_totp(code)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit FIDO2 authentication data.
    pub async fn submit_fido(
        &self,
        fido_data: Fido2RequestFfi,
    ) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_fido(fido_data.into())
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Change the account password; leaves the mailbox password (if any) unchanged.
    pub async fn change_pass(
        &self,
        current_password: String,
        new_pass: String,
        confirm_password: String,
        token: Option<Arc<PasswordValidatorServiceToken>>,
    ) -> Result<SimplePasswordState, PasswordError> {
        check_password(PasswordType::Main, &new_pass, &confirm_password, token)?;

        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .change_pass(current_password, new_pass)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Change the mailbox password; leaves the account password unchanged.
    pub async fn change_mbox_pass(
        &self,
        current_password: String,
        new_mbox_pass: String,
        confirm_password: String,
        token: Option<Arc<PasswordValidatorServiceToken>>,
    ) -> Result<SimplePasswordState, PasswordError> {
        check_password(
            PasswordType::Secondary,
            &new_mbox_pass,
            &confirm_password,
            token,
        )?;

        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .change_mbox_pass(current_password, new_mbox_pass)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Returns a password validator service.
    #[must_use]
    pub async fn password_validator(&self) -> Option<Arc<PasswordValidatorService>> {
        let flow = self.flow.clone();

        let result: Result<_, PasswordError> = uniffi_async(async move {
            let api = flow.lock().await.api();
            Ok(Arc::new(PasswordValidatorService::setup(api.into_dyn())))
        })
        .await;
        result.ok()
    }

    /// Get whether the account has TOTP enabled.
    pub fn has_totp(&self) -> Result<bool, PasswordError> {
        async_runtime().block_on(async { Ok(self.flow.lock().await.has_totp()) })
    }

    /// Get whether the account has FIDO2 enabled.
    pub fn has_fido(&self) -> Result<bool, PasswordError> {
        async_runtime().block_on(async { Ok(self.flow.lock().await.has_fido()) })
    }

    /// Get the FIDO2 details for authentication.
    pub async fn fido_details(&self) -> Result<Option<Fido2ResponseFfi>, PasswordError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .fido_details()
                .await
                .map(|res| res.map(Fido2ResponseFfi::from))
                .map_err(PasswordError::from)
        })
        .await
    }

    /// Get whether the account has a mailbox password.
    pub fn has_mbp(&self) -> Result<bool, PasswordError> {
        async_runtime().block_on(async { Ok(self.flow.lock().await.has_mbp()) })
    }

    /// Step the flow back to the previous state.
    pub async fn step_back(&self) -> Result<SimplePasswordState, PasswordError> {
        let flow = self.flow.clone();

        let _: Result<_, PasswordError> = uniffi_async(async move {
            flow.lock().await.back();
            Ok(())
        })
        .await;

        Ok(self.get_state())
    }
}

fn check_password(
    password_type: PasswordType,
    password: &String,
    confirm_password: &String,
    token: Option<Arc<PasswordValidatorServiceToken>>,
) -> Result<(), PasswordError> {
    if password.trim().is_empty() {
        return Err(PasswordError::PasswordEmpty);
    }

    if password != confirm_password {
        return Err(PasswordError::PasswordsNotMatching);
    }

    token
        .ok_or(PasswordError::PasswordNotValidated)?
        .matches(password_type, password)
        .then_some(())
        .ok_or(PasswordError::PasswordValidationMismatch)
}
