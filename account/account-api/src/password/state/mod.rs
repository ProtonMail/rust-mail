use crate::password::PasswordError;
use crate::password::state::complete::Complete;
use crate::password::state::want_change::WantChange;
use crate::password::state::want_pass::WantPass;
use crate::password::state::want_tfa::WantTfa;
use crate::shared::SecureString;
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
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        client: Client,
        parts: SessionParts,
        username: String,
        user_keys: UserKeys,
        key_secret: KeySecret,
        tfa_mode: TfaStatus,
        mbp_mode: PasswordMode,
    ) -> Result<Self, PasswordError> {
        let request = PostAuthInfoRequest {
            username: username.clone(),
        };

        let auth_info = client
            .post_auth_info(request)
            .map_err(PasswordError::ApiService)
            .await?;

        let data = StateData {
            client,
            parts,
            username,
            user_keys,
            key_secret,
            tfa_mode,
            mbp_mode,
            auth_info,
        };

        Ok(WantPass::new(data).into())
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

    #[must_use]
    pub fn kind(&self) -> StateKind {
        StateKind::of(self)
    }

    /// Get whether the account has TOTP enabled.
    pub fn has_totp(&self) -> Result<bool, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(state.tfa_mode.has_totp()),
            Self::WantTfa(state) => Ok(state.tfa_mode.has_totp()),
            Self::WantChange(state) => Ok(state.tfa_mode.has_totp()),
            _ => Err(PasswordError::InvalidState),
        }
    }

    /// Get whether the account has a mailbox password.
    pub fn has_mbp(&self) -> Result<bool, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(state.mbp_mode.has_mbp()),
            Self::WantTfa(state) => Ok(state.mbp_mode.has_mbp()),
            Self::WantChange(state) => Ok(state.mbp_mode.has_mbp()),
            _ => Err(PasswordError::InvalidState),
        }
    }

    /// Get whether the account has FIDO2 enabled.
    pub fn has_fido(&self) -> Result<bool, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(state.tfa_mode.has_fido()),
            Self::WantTfa(state) => Ok(state.tfa_mode.has_fido()),
            Self::WantChange(state) => Ok(state.tfa_mode.has_fido()),
            _ => Err(PasswordError::InvalidState),
        }
    }

    /// Get the FIDO2 details for authentication.
    pub fn fido_details(&self) -> Result<Option<fido2::Response>, PasswordError> {
        let info = match self {
            Self::WantPass(state) => &state.auth_info,
            Self::WantTfa(state) => &state.auth_info,
            Self::WantChange(state) => &state.auth_info,
            _ => return Err(PasswordError::InvalidState),
        };

        match &info.tfa {
            Some(tfa) => Ok(tfa.fido_details()),
            None => Ok(None),
        }
    }

    /// Get the API client for external operations.
    pub fn api(&self) -> Result<&Client, PasswordError> {
        match self {
            Self::WantPass(state) => Ok(&state.client),
            Self::WantTfa(state) => Ok(&state.client),
            Self::WantChange(state) => Ok(&state.client),
            Self::Complete(state) => Ok(state.client()),
            Self::Invalid => Err(PasswordError::InvalidState),
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
    auth_info: PostAuthInfoResponse,
}

async fn acquire_password_scope(
    srp: &impl SRPProvider,
    client: &Client,
    auth_info: &PostAuthInfoResponse,
    username: &str,
    password: &SecureString,
    totp: Option<String>,
    fido2_data: Option<fido2::Request>,
) -> Result<PutUsersPasswordResponse, PasswordError> {
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
        two_factor_code: totp,
        fido2: fido2_data,
        sso_reauth_token: None,
    };

    let response = client.put_users_password(request).await?;

    if response.server_proof == client_proof.expected_server_proof {
        Ok(response)
    } else {
        Err(PasswordError::ServerProof)
    }
}
