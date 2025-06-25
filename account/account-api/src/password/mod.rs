use crate::ApiError;
use crate::password::state::State;
use proton_core_api::service::{ApiServiceError, ServiceError};
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::Session;
use proton_core_api::store::StoreError;
use proton_core_common::datatypes::TfaStatus;
use proton_crypto_account::keys::{LockedKey, UserKeys};
use proton_crypto_account::proton_crypto::CryptoError;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::future::Future;
use std::string::FromUtf8Error;
use thiserror::Error;

/// Alias the `SaltError` as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

/// Implements the possible states that the password change flow can be in.
pub mod state;

/// Errors that can occur during the password change flow.
#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("API error: {0}")]
    Api(#[source] ApiError),

    #[error("API service error: {0}")]
    ApiService(#[source] ApiServiceError),

    #[error("Failed to authenticate: {0}")]
    FlowAuth(#[source] ApiServiceError),

    #[error("Failed to verify server proof")]
    ServerProof,

    #[error("Missing primary key")]
    MissingPrimaryKey,

    #[error("Failed to fetch key salts: {0}")]
    KeySecretSaltFetch(#[source] ApiServiceError),

    #[error("Failed to derive the key secret from the password: {0}")]
    KeySecretDerivation(#[from] SaltError),

    #[error("Failed to decrypt a user key with the derived client secret")]
    KeySecretDecryption,

    #[error("Failed to convert key bytes to UTF-8 string")]
    KeyEncoding(#[from] FromUtf8Error),

    #[error("Crypto: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Store: {0}")]
    Store(#[from] StoreError),

    #[error("Invalid state")]
    InvalidState,
}

impl ServiceError for PasswordError {}

/// A password change flow that can be used to change a user's password.
///
/// The flow is used to guide the user through the password change process,
/// ensuring that all necessary steps are completed in the correct order.
#[derive(Debug)]
pub struct PasswordFlow {
    state: State,
}

impl PasswordFlow {
    /// Create a new password change flow.
    ///
    /// # Arguments
    /// * `session` - The authenticated session
    /// * `tfa_status` - The user's 2FA requirements
    #[must_use]
    pub fn new(session: impl Borrow<Session>, tfa_status: TfaStatus) -> Self {
        let (client, parts) = session.borrow().to_parts();

        let state = State::new(client, parts, tfa_status);

        Self { state }
    }

    /// Submit current password.
    ///
    /// # Errors
    ///
    /// Returns error if the password submission fails.
    pub async fn submit_password(&mut self, password: String) -> Result<(), PasswordError> {
        self.transition(|s: State| s.submit_password(password))
            .await
    }

    /// Submit TOTP code for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the TOTP code submission fails.
    pub async fn submit_totp(&mut self, code: String) -> Result<(), PasswordError> {
        self.transition(|s: State| s.submit_totp(code)).await
    }

    /// Submit FIDO2 response for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the FIDO2 response submission fails.
    pub async fn submit_fido2(&mut self, response: String) -> Result<(), PasswordError> {
        self.transition(|s: State| s.submit_fido(response)).await
    }

    /// Submit new password.
    ///
    /// # Errors
    ///
    /// Returns error if the password change request or crypto operations failed.
    pub async fn submit_new_password(&mut self, new_pass: String) -> Result<(), PasswordError> {
        self.transition(|s: State| s.submit_new_password(new_pass))
            .await
    }

    /// Take the completed session from the flow.
    ///
    /// # Errors
    ///
    /// Returns error if the flow is not complete.
    pub fn take_session(&mut self) -> Result<Session, PasswordError> {
        let state = std::mem::take(&mut self.state);
        state.into_session()
    }

    /// Check if the flow is awaiting password.
    #[must_use]
    pub fn is_awaiting_password(&self) -> bool {
        matches!(self.state, State::WantPassword(_))
    }

    /// Check if the flow is awaiting 2FA.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, State::WantTfa(_))
    }

    /// Check if the flow is awaiting new password.
    #[must_use]
    pub fn is_awaiting_new_password(&self) -> bool {
        matches!(self.state, State::WantNewPassword(_))
    }

    /// Check if the flow is complete.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self.state, State::Complete(_))
    }

    /// Get the API client for external operations.
    #[must_use]
    pub fn api(&self) -> &muon::Client {
        self.state.api()
    }

    async fn transition<F, Fut>(&mut self, f: F) -> Result<(), PasswordError>
    where
        F: FnOnce(State) -> Fut,
        Fut: Future<Output = Result<State, PasswordError>>,
    {
        let state = std::mem::take(&mut self.state);

        match f(state).await {
            Ok(new_state) => {
                self.state = new_state;
                Ok(())
            }
            Err(err) => {
                self.state = State::Invalid;
                Err(err)
            }
        }
    }
}

// Extension trait for UserKeys to find primary key
trait UserKeysExt {
    fn primary(&self) -> Option<&LockedKey>;
}

impl UserKeysExt for UserKeys {
    fn primary(&self) -> Option<&LockedKey> {
        self.as_ref().iter().find(|&key| key.primary)
    }
}

#[cfg(test)]
mod tests {
    use proton_core_common::datatypes::TfaStatus;

    #[test]
    fn test_tfa_status_want_tfa() {
        // Test TFA status want_tfa method

        // Test with no 2FA
        let tfa_none = TfaStatus::None;
        assert!(!tfa_none.want_tfa());

        // Test with TOTP 2FA
        let tfa_totp = TfaStatus::Totp;
        assert!(tfa_totp.want_tfa());

        // Test with FIDO2 2FA
        let tfa_fido = TfaStatus::Fido2;
        assert!(tfa_fido.want_tfa());

        // Test with both TOTP and FIDO2 2FA
        let tfa_both = TfaStatus::TotpOrFido2;
        assert!(tfa_both.want_tfa());
    }
}
