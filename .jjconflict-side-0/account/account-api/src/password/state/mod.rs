use crate::password::state::want_tfa::WantTfa;
use crate::shared::SecureString;
use derive_more::{Debug, Display, From};
use muon::Client;
use proton_core_api::services::proton::prelude::*;
use proton_core_api::{auth::KeySecret, session::SessionParts};
use proton_core_common::datatypes::{PasswordMode, TfaStatus};
use proton_crypto_account::keys::UserKeys;

pub mod want_tfa;

/// Represents the possible states that the password change flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(Debug, Default, From, Clone, Copy)]
pub enum State {
    /// The flow is waiting for the user to provide a 2FA token.
    #[debug("WantTfa")]
    WantTfa(WantTfa),

    /// The flow is waiting for the user to provide their new password / mailbox password.
    #[debug("WantChange")]
    WantChange,

    /// The flow is complete.
    #[debug("Complete")]
    Complete,

    /// Invalid state, cannot be used.
    #[default]
    #[debug("Invalid")]
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    #[must_use]
    pub fn kind(self) -> StateKind {
        StateKind::of(self)
    }
}

/// Represents the different kinds of states in the password change flow.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
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
    fn of(state: State) -> Self {
        match state {
            State::WantTfa(_) => Self::WantTfa,
            State::WantChange => Self::WantChange,
            State::Complete => Self::Complete,
            State::Invalid => Self::Invalid,
        }
    }
}

/// Shared data between states.
#[derive(Clone)]
pub struct StateData {
    pub client: Client,
    pub parts: SessionParts,
    pub username: String,
    pub current_password: SecureString,
    pub new_password: SecureString,
    pub user_keys: UserKeys,
    pub key_secret: KeySecret,
    pub tfa_mode: TfaStatus,
    pub mbp_mode: PasswordMode,
    pub auth_info: Option<PostAuthInfoResponse>,
}
