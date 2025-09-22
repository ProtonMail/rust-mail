use crate::ApiError;
use crate::password::api::PasswordScope;
use crate::password::observability::{ObservableResult, ObservableState};

use crate::password::state::want_tfa::WantTfa;
use crate::password::state::{State, StateData, StateKind};
use crate::shared::SecureString;
use crate::shared::challenge::get_auth_info;
use futures::TryFutureExt as _;
use muon::Status;
use muon::rest::auth::v4::fido2;
use proton_core_api::auth::KeySecret;
use proton_core_api::consts::CoreBundle;
use proton_core_api::service::{ApiServiceError, ServiceError};
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::Session;
use proton_core_api::store::StoreError;
use proton_core_common::datatypes::{PasswordMode, TfaStatus};
use proton_core_common::observability::PreLoginMetricRecorder;
use proton_crypto_account::keys::UserKeys;
use proton_crypto_account::proton_crypto::{CryptoError, new_srp_provider};
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::string::FromUtf8Error;
use thiserror::Error;

/// Alias the `SaltError` as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

mod api;
/// Implements the possible states that the password change flow can be in.
pub mod state;

mod observability;

/// Auth errors that can occur during the password change flow.
#[derive(Debug, Error)]
pub enum FlowAuthError {
    #[error("Password wrong: {0:?}")]
    PasswordWrong(#[from] PasswordWrongDetails),

    #[error("Auth error: {0} ({})", _0.err_info().map(|i| i.to_string()).unwrap_or_default())]
    Other(#[from] ApiError),
}

/// Details provided by the backend in case of `PasswordWrong` error.
#[derive(Debug, Error, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[error(transparent)]
pub struct PasswordWrongDetails {
    pub login_failed_reason: LoginFailedReason,
}

/// Specific reasons why a `PasswordWrong` error was returned by the backend.
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum LoginFailedReason {
    #[error("Wrong 2FA code")]
    TotpWrong,
    #[error("2FA code already used")]
    TotpReuse,
    #[error("Wrong recovery phrase")]
    RecoveryPhrase,
    #[serde(other)]
    #[error("Other")]
    Other,
}

/// Errors that can occur during the password change flow.
#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("API error: {0} ({})", _0.err_info().map(|i| i.to_string()).unwrap_or_default())]
    Api(#[source] ApiError),

    #[error("API service error: {0}")]
    ApiService(#[source] ApiServiceError),

    #[error("Auth error: {0}")]
    FlowAuth(#[from] FlowAuthError),

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

impl From<ApiError> for PasswordError {
    fn from(api_error: ApiError) -> Self {
        if api_error.err_status() == Some(Status::UNPROCESSABLE_ENTITY)
            && api_error.err_code() == Some(CoreBundle::PasswordWrong as u32)
        {
            api_error
                .err_info()
                .and_then(|info| info.details)
                .and_then(|value| serde_json::from_value::<PasswordWrongDetails>(value).ok())
                .map(FlowAuthError::from)
                .map(PasswordError::from)
        } else {
            None
        }
        .unwrap_or_else(|| PasswordError::FlowAuth(FlowAuthError::Other(api_error)))
    }
}

/// A password change flow that can be used to change a user's password.
///
/// The flow is used to guide the user through the password change process,
/// ensuring that all necessary steps are completed in the correct order.
pub struct PasswordFlow {
    state: State,
    data: StateData,
    recorder: PreLoginMetricRecorder,
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
    pub fn new(
        session: impl Borrow<Session>,
        username: String,
        user_keys: UserKeys,
        key_secret: KeySecret,
        tfa_mode: TfaStatus,
        mbp_mode: PasswordMode,
    ) -> Self {
        let (client, parts) = session.borrow().to_parts();

        let data = StateData {
            client,
            parts,
            username,
            current_password: SecureString::from(String::new()),
            new_password: SecureString::from(String::new()),
            user_keys,
            key_secret,
            tfa_mode,
            mbp_mode,
            auth_info: None,
        };

        let state = State::WantChange;

        Self {
            data,
            state,
            recorder: PreLoginMetricRecorder::default(),
        }
    }

    /// Get the kind of the current state.
    pub fn kind(&self) -> Result<StateKind, PasswordError> {
        Ok(self.state.kind())
    }

    /// Submit TOTP code for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the TOTP code submission fails.
    pub async fn submit_totp(&mut self, totp: String) -> Result<(), PasswordError> {
        match self.state {
            State::WantTfa(want_tfa) => {
                let pw_scope = submit_totp(&mut self.data, totp).await?;

                let observable_data = self.data.observable_data();
                self.state = if want_tfa.change_master_password {
                    pw_scope
                        .change_mbox_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                } else {
                    pw_scope
                        .change_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                };
            }
            State::WantChange | State::Complete | State::Invalid => {
                return Err(PasswordError::InvalidState);
            }
        }
        Ok(())
    }

    /// Submit FIDO2 data for 2FA authentication.
    ///
    /// # Errors
    ///
    /// Returns error if the FIDO2 submission fails.
    pub async fn submit_fido(&mut self, fido_data: fido2::Request) -> Result<(), PasswordError> {
        match self.state {
            State::WantTfa(want_tfa) => {
                let pw_scope = submit_fido(&mut self.data, fido_data).await?;

                let observable_data = self.data.observable_data();
                self.state = if want_tfa.change_master_password {
                    pw_scope
                        .change_mbox_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                } else {
                    pw_scope
                        .change_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                };
            }
            State::WantChange | State::Complete | State::Invalid => {
                return Err(PasswordError::InvalidState);
            }
        }

        Ok(())
    }

    /// Change the account password.
    ///
    /// # Errors
    ///
    /// Returns error if the password change request or crypto operations failed.
    pub async fn change_pass(
        &mut self,
        current_pass: impl Into<SecureString>,
        new_pass: impl Into<SecureString>,
    ) -> Result<(), PasswordError> {
        let observable_data = self.data.observable_data();
        self.data.current_password = current_pass.into();
        self.data.new_password = new_pass.into();

        match self.state {
            State::WantChange => {
                self.state = if self.data.tfa_mode.has_tfa() {
                    WantTfa::for_changing_password().into()
                } else {
                    let pw_scope = PasswordScope::acquire(
                        &new_srp_provider(),
                        &self.data.client,
                        &self.data.username,
                        &self.data.current_password,
                        self.data.auth_info.take(),
                        None,
                        None,
                    )
                    .await?;
                    pw_scope
                        .change_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                }
            }
            State::WantTfa(_) | State::Complete | State::Invalid => {
                return Err(PasswordError::InvalidState);
            }
        }

        Ok(())
    }

    /// Change the mailbox password.
    ///
    /// # Errors
    ///
    /// Returns error if the mailbox password change request or crypto operations failed.
    pub async fn change_mbox_pass(
        &mut self,
        current_pass: impl Into<SecureString>,
        new_mbox_pass: impl Into<SecureString>,
    ) -> Result<(), PasswordError> {
        let observable_data = self.data.observable_data();
        self.data.current_password = current_pass.into();
        self.data.new_password = new_mbox_pass.into();

        match self.state {
            State::WantChange => {
                self.state = if self.data.tfa_mode.has_tfa() {
                    WantTfa::for_changing_master_password().into()
                } else {
                    let pw_scope = PasswordScope::acquire(
                        &new_srp_provider(),
                        &self.data.client,
                        &self.data.username,
                        &self.data.current_password,
                        self.data.auth_info.take(),
                        None,
                        None,
                    )
                    .await?;
                    pw_scope
                        .change_mbox_pass(&self.data)
                        .await
                        .observe(&self.recorder, observable_data)?
                };
            }
            State::WantTfa(_) | State::Complete | State::Invalid => {
                return Err(PasswordError::InvalidState);
            }
        }

        Ok(())
    }

    /// Get the FIDO2 details for authentication.
    pub async fn fido_details(&mut self) -> Result<Option<fido2::Response>, PasswordError> {
        let info = if let Some(info) = &self.data.auth_info {
            info
        } else {
            self.data.auth_info.insert(
                get_auth_info(&self.data.client, &self.data.username)
                    .map_err(PasswordError::ApiService)
                    .await?,
            )
        };

        Ok(info.fido_details())
    }

    /// Get whether the account has TOTP enabled.
    #[must_use]
    pub fn has_totp(&self) -> bool {
        self.data.tfa_mode.has_totp()
    }

    /// Get whether the account has FIDO2 enabled.
    #[must_use]
    pub fn has_fido(&self) -> bool {
        self.data.tfa_mode.has_fido()
    }

    /// Get whether the account has a mailbox password.
    #[must_use]
    pub fn has_mbp(&self) -> bool {
        self.data.mbp_mode.has_mbp()
    }

    /// Get the API client for external operations.
    #[must_use]
    pub fn api(&self) -> muon::Client {
        self.data.client.clone()
    }

    /// Return to the previous state. In case of this flow, it means
    /// starting the flow from the beginning
    pub fn back(&mut self) {
        self.state = State::WantChange;
    }
}
pub async fn submit_totp(
    data: &mut StateData,
    code: String,
) -> Result<PasswordScope, PasswordError> {
    PasswordScope::acquire(
        &new_srp_provider(),
        &data.client,
        &data.username,
        &data.current_password,
        data.auth_info.take(),
        Some(code),
        None,
    )
    .await
}

pub async fn submit_fido(
    data: &mut StateData,
    fido_data: fido2::Request,
) -> Result<PasswordScope, PasswordError> {
    PasswordScope::acquire(
        &new_srp_provider(),
        &data.client,
        &data.username,
        &data.current_password,
        data.auth_info.take(),
        None,
        Some(fido_data),
    )
    .await
}
