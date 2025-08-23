use crate::password::PasswordError;
use crate::password::state::complete::Complete;
use crate::password::state::want_change::WantChange;
use crate::password::state::want_pass::WantPass;
use crate::password::state::want_tfa::WantTfa;
use crate::shared::SecureString;
use crate::shared::challenge::get_auth_info;
use crate::{AccountApi, prelude::*};
use derive_more::{Debug, Display, From};
use futures::TryFutureExt;
use muon::Client;
use muon::rest::auth::v4::fido2;
use proton_core_api::services::proton::prelude::*;
use proton_core_api::{auth::KeySecret, session::SessionParts};
use proton_core_common::datatypes::{PasswordMode, TfaStatus};
use proton_crypto_account::keys::UserKeys;
use proton_crypto_account::proton_crypto::srp::SRPProvider;

pub mod complete;
pub mod want_change;
pub mod want_pass;
pub mod want_tfa;

/// Represents the possible states that the password change flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(Debug, Default, From, Clone)]
pub enum State {
    /// The flow is waiting for the user to provide their current password.
    #[debug("WantPass")]
    WantPass(WantPass),

    /// The flow is waiting for the user to provide a 2FA token.
    #[debug("WantTfa")]
    WantTfa(WantTfa),

    /// The flow is waiting for the user to provide their new password / mailbox password.
    #[debug("WantChange")]
    WantChange(WantChange),

    /// The flow is complete.
    #[debug("Complete")]
    Complete(Complete),

    /// Invalid state, cannot be used.
    #[default]
    #[debug("Invalid")]
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    /// Create a new state machine with current password.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        parts: SessionParts,
        username: String,
        user_keys: UserKeys,
        key_secret: KeySecret,
        tfa_mode: TfaStatus,
        mbp_mode: PasswordMode,
    ) -> Self {
        let data = StateData {
            client,
            parts,
            username,
            user_keys,
            key_secret,
            tfa_mode,
            mbp_mode,
            auth_info: None,
        };

        WantPass::new(data).into()
    }

    #[must_use]
    pub fn kind(&self) -> StateKind {
        StateKind::of(self)
    }

    /// Submit current password.
    pub async fn submit_pass(self, pass: SecureString) -> Result<Self, PasswordError> {
        if let Self::WantPass(state) = self {
            state.submit_pass(pass).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit TOTP code for 2FA authentication.
    pub async fn submit_totp(self, totp: String) -> Result<Self, PasswordError> {
        if let Self::WantTfa(state) = self {
            state.submit_totp(totp).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit FIDO2 data for 2FA authentication.
    pub async fn submit_fido(self, fido_data: fido2::Request) -> Result<Self, PasswordError> {
        if let Self::WantTfa(state) = self {
            state.submit_fido(fido_data).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit new password.
    pub async fn change_pass(self, new_pass: SecureString) -> Result<Self, PasswordError> {
        if let Self::WantChange(state) = self {
            state.change_pass(new_pass).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit new mailbox password (if MBP is enabled).
    pub async fn change_mbox_pass(
        self,
        new_mbox_pass: SecureString,
    ) -> Result<Self, PasswordError> {
        if let Self::WantChange(state) = self {
            state.change_mbox_pass(new_mbox_pass).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Get whether the account has TOTP enabled.
    pub fn has_totp(&self) -> Result<bool, PasswordError> {
        Ok(self.data_ref()?.tfa_mode.has_totp())
    }

    /// Get whether the account has a mailbox password.
    pub fn has_mbp(&self) -> Result<bool, PasswordError> {
        Ok(self.data_ref()?.mbp_mode.has_mbp())
    }

    /// Get whether the account has FIDO2 enabled.
    pub fn has_fido(&self) -> Result<bool, PasswordError> {
        Ok(self.data_ref()?.tfa_mode.has_fido())
    }

    /// Get the FIDO2 details for authentication.
    pub async fn fido_details(&mut self) -> Result<Option<fido2::Response>, PasswordError> {
        let data = self.data_mut()?;

        if data.auth_info.is_none() {
            data.auth_info = Some(
                get_auth_info(&data.client, &data.username)
                    .map_err(PasswordError::ApiService)
                    .await?,
            );
        }

        let Some(info) = &data.auth_info else {
            unreachable!()
        };

        Ok(info.fido_details())
    }

    /// Get the API client for external operations.
    pub fn api(&self) -> Result<&Client, PasswordError> {
        if let Ok(data) = self.data_ref() {
            return Ok(&data.client);
        }

        if let Self::Complete(state) = self {
            return Ok(state.client());
        }

        Err(PasswordError::InvalidState)
    }

    fn data_ref(&self) -> Result<&StateData, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(state),
            Self::WantTfa(state) => Ok(state),
            Self::WantChange(state) => Ok(state),
            _ => Err(PasswordError::InvalidState),
        }
    }

    fn data_mut(&mut self) -> Result<&mut StateData, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(state),
            Self::WantTfa(state) => Ok(state),
            Self::WantChange(state) => Ok(state),
            _ => Err(PasswordError::InvalidState),
        }
    }
}

/// Represents the different kinds of states in the password change flow.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
    /// Waiting for password input.
    WantPass,
    /// Waiting for two-factor authentication.
    WantTfa,
    /// Ready to change password.
    WantChange,
    /// Password change completed successfully.
    Complete,
    /// Invalid state.
    Invalid,
}

impl StateKind {
    fn of(state: &State) -> Self {
        match state {
            State::WantPass(_) => Self::WantPass,
            State::WantTfa(_) => Self::WantTfa,
            State::WantChange(_) => Self::WantChange,
            State::Complete(_) => Self::Complete,
            State::Invalid => Self::Invalid,
        }
    }
}

/// Shared data between states.
#[derive(Clone)]
pub struct StateData {
    client: Client,
    parts: SessionParts,
    username: String,
    user_keys: UserKeys,
    key_secret: KeySecret,
    tfa_mode: TfaStatus,
    mbp_mode: PasswordMode,
    auth_info: Option<PostAuthInfoResponse>,
}

async fn acquire_password_scope(
    srp: &impl SRPProvider,
    client: &Client,
    username: &str,
    password: &SecureString,
    auth_info: Option<PostAuthInfoResponse>,
    two_factor_code: Option<String>,
    fido2: Option<fido2::Request>,
) -> Result<PutUsersPasswordResponse, PasswordError> {
    let auth_info = match (auth_info, fido2.is_some()) {
        (Some(info), _) => info,
        (None, true) => return Err(PasswordError::InvalidState),
        (None, false) => {
            get_auth_info(client, username)
                .map_err(PasswordError::ApiService)
                .await?
        }
    };

    let client_proof = srp.generate_client_proof(
        username,
        password,
        auth_info.version,
        &auth_info.salt,
        &auth_info.modulus,
        &auth_info.server_ephemeral,
    )?;

    let request = PutUsersPasswordRequest {
        client_ephemeral: client_proof.ephemeral,
        client_proof: client_proof.proof,
        srp_session: auth_info.session.clone(),
        two_factor_code,
        fido2,
    };

    let response = client.put_users_password(request).await?;

    if response.server_proof == client_proof.expected_server_proof {
        Ok(response)
    } else {
        Err(PasswordError::ServerProof)
    }
}
