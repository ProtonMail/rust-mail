use crate::ApiError;
use crate::password::state::{State, StateKind};
use proton_core_api::auth::KeySecret;
use proton_core_api::service::{ApiServiceError, ServiceError};
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::Session;
use proton_core_api::store::StoreError;
use proton_core_common::datatypes::{PasswordMode, TfaStatus};
use proton_crypto_account::keys::UserKeys;
use proton_crypto_account::proton_crypto::CryptoError;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::string::FromUtf8Error;
use thiserror::Error;

/// Alias the `SaltError` as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

/// Implements the possible states that the password change flow can be in.
pub mod state;

/// Errors that can occur during the password change flow.
#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("API error: {0} ({})", _0.err_info().map(|i| i.to_string()).unwrap_or_default())]
    Api(#[source] ApiError),

    #[error("API service error: {0}")]
    ApiService(#[source] ApiServiceError),

    #[error("Auth error: {0} ({})", _0.err_info().map(|i| i.to_string()).unwrap_or_default())]
    FlowAuth(#[source] ApiError),

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
    state: Vec<State>,
}

impl PasswordFlow {
    /// Create a new password change flow.
    ///
    /// # Arguments
    /// * `session` - The authenticated session
    /// * `tfa_mode` - The 2FA mode
    /// * `mbp_mode` - The mailbox password mode
    /// * `key_secret` - The key secret
    #[must_use]
    pub fn new(
        session: impl Borrow<Session>,
        username: String,
        user_keys: UserKeys,
        key_secret: KeySecret,
        tfa_mode: TfaStatus,
        mbp_mode: PasswordMode,
    ) -> Self {
        let (client, parts) = session.borrow().to_parts();

        let state = State::new(
            client, parts, username, user_keys, key_secret, tfa_mode, mbp_mode,
        );

        Self { state: vec![state] }
    }

    /// Submit current password.
    ///
    /// # Errors
    ///
    /// Returns error if the password submission fails.
    pub async fn submit_pass(&mut self, pass: String) -> Result<(), PasswordError> {
        let next = self.state()?.submit_pass(pass).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit TOTP code for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the TOTP code submission fails.
    pub async fn submit_totp(&mut self, totp: String) -> Result<(), PasswordError> {
        let next = self.state()?.submit_totp(totp).await?;

        self.state.push(next);

        Ok(())
    }

    /// Change the account password.
    ///
    /// # Errors
    ///
    /// Returns error if the password change request or crypto operations failed.
    pub async fn change_pass(&mut self, new_pass: String) -> Result<(), PasswordError> {
        let next = self.state()?.change_pass(new_pass).await?;

        self.state.push(next);

        Ok(())
    }

    /// Change the mailbox password.
    ///
    /// # Errors
    ///
    /// Returns error if the mailbox password change request or crypto operations failed.
    pub async fn change_mbox_pass(&mut self, new_mbox_pass: String) -> Result<(), PasswordError> {
        let next = self.state()?.change_mbox_pass(new_mbox_pass).await?;

        self.state.push(next);

        Ok(())
    }

    /// Get the kind of the current state.
    pub fn kind(&self) -> Result<StateKind, PasswordError> {
        Ok(self.state()?.kind())
    }

    /// Get the mailbox password mode.
    pub fn mbp_mode(&self) -> Result<PasswordMode, PasswordError> {
        self.state()?.mbp_mode()
    }

    /// Get the API client for external operations.
    pub fn api(&self) -> Result<muon::Client, PasswordError> {
        Ok(self.state()?.api()?.to_owned())
    }

    /// Return to the previous state.
    pub fn back(&mut self) -> Result<(), PasswordError> {
        if self.state.len() < 2 {
            return Err(PasswordError::InvalidState);
        }

        self.state.pop();

        Ok(())
    }

    fn state(&self) -> Result<State, PasswordError> {
        self.state
            .last()
            .cloned()
            .ok_or(PasswordError::InvalidState)
    }
}
