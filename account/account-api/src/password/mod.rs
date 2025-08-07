use crate::ApiError;
use crate::password::observability::{ObservableResult, ObservableState};
use crate::password::state::{State, StateKind};
use crate::shared::SecureString;
use muon::rest::auth::v4::fido2;
use proton_core_api::auth::KeySecret;
use proton_core_api::service::{ApiServiceError, ServiceError};
use proton_core_api::services::observability::ObservabilityRecorder;
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::Session;
use proton_core_api::store::StoreError;
use proton_core_common::datatypes::{PasswordMode, TfaStatus};
use proton_crypto_account::keys::UserKeys;
use proton_crypto_account::proton_crypto::CryptoError;
use std::borrow::Borrow;
use std::string::FromUtf8Error;
use thiserror::Error;

/// Alias the `SaltError` as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

/// Implements the possible states that the password change flow can be in.
pub mod state;

mod observability;

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
    recorder: ObservabilityRecorder,
}

impl PasswordFlow {
    /// Create a new password change flow.
    ///
    /// # Arguments
    ///
    /// * `session` - The API session
    /// * `username` - The username of the user
    /// * `user_keys` - The user's keys
    /// * `key_secret` - The key secret
    /// * `tfa_mode` - The 2FA mode
    /// * `mbp_mode` - The mailbox password mode
    pub async fn new(
        session: impl Borrow<Session>,
        username: String,
        user_keys: UserKeys,
        key_secret: KeySecret,
        tfa_mode: TfaStatus,
        mbp_mode: PasswordMode,
    ) -> Result<Self, PasswordError> {
        let (client, parts) = session.borrow().to_parts();

        let state = State::new(
            client, parts, username, user_keys, key_secret, tfa_mode, mbp_mode,
        )
        .await?;

        Ok(Self {
            state: vec![state],
            recorder: ObservabilityRecorder::default(),
        })
    }

    /// Submit current password.
    ///
    /// # Errors
    ///
    /// Returns error if the password submission fails.
    pub async fn submit_pass(
        &mut self,
        pass: impl Into<SecureString>,
    ) -> Result<(), PasswordError> {
        let state = self.state()?;
        let observable_data = state.observable_data();
        let next = state
            .submit_pass(pass.into())
            .await
            .observe(&self.recorder, observable_data)?;

        self.state.push(next);

        Ok(())
    }

    /// Submit TOTP code for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the TOTP code submission fails.
    pub async fn submit_totp(&mut self, totp: String) -> Result<(), PasswordError> {
        let state = self.state()?;
        let observable_data = state.observable_data();
        let next = state
            .submit_totp(totp)
            .await
            .observe(&self.recorder, observable_data)?;

        self.state.push(next);

        Ok(())
    }

    /// Submit FIDO2 data for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the FIDO2 submission fails.
    pub async fn submit_fido(&mut self, fido_data: fido2::Request) -> Result<(), PasswordError> {
        let next = self.state()?.submit_fido(fido_data).await?;

        self.state.push(next);

        Ok(())
    }

    /// Change the account password.
    ///
    /// # Errors
    ///
    /// Returns error if the password change request or crypto operations failed.
    pub async fn change_pass(
        &mut self,
        new_pass: impl Into<SecureString>,
    ) -> Result<(), PasswordError> {
        let state = self.state()?;
        let observable_data = state.observable_data();
        let next = state
            .change_pass(new_pass.into())
            .await
            .observe(&self.recorder, observable_data)?;

        self.state.push(next);

        Ok(())
    }

    /// Change the mailbox password.
    ///
    /// # Errors
    ///
    /// Returns error if the mailbox password change request or crypto operations failed.
    pub async fn change_mbox_pass(
        &mut self,
        new_mbox_pass: impl Into<SecureString>,
    ) -> Result<(), PasswordError> {
        let state = self.state()?;
        let observable_data = state.observable_data();
        let next = state
            .change_mbox_pass(new_mbox_pass.into())
            .await
            .observe(&self.recorder, observable_data)?;

        self.state.push(next);

        Ok(())
    }

    /// Get the kind of the current state.
    pub fn kind(&self) -> Result<StateKind, PasswordError> {
        Ok(self.state()?.kind())
    }

    /// Get whether the account has TOTP enabled.
    pub fn has_totp(&self) -> Result<bool, PasswordError> {
        self.state()?.has_totp()
    }

    /// Get whether the account has FIDO2 enabled.
    pub fn has_fido(&self) -> Result<bool, PasswordError> {
        self.state()?.has_fido()
    }

    /// Get the FIDO2 details for authentication.
    pub fn get_fido_details(&self) -> Result<Option<fido2::Response>, PasswordError> {
        self.state()?.fido_details()
    }

    /// Get whether the account has a mailbox password.
    pub fn has_mbp(&self) -> Result<bool, PasswordError> {
        self.state()?.has_mbp()
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
