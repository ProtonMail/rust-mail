use crate::prelude::Address;
use crate::shared::SecureString;
use crate::signup::state::want_create::WantCreate;
use crate::signup::state::want_password::WantPassword;
use crate::signup::state::want_recovery::WantRecovery;
use crate::signup::state::want_username::WantUsername;
use crate::signup::{Behavior, ChallengeInfo};
use crate::{prelude::User, signup::SignupError};
use complete::Complete;
use derive_more::{Display, From, TryInto};
use muon::Client;
use proton_core_api::store::DynStore;
use proton_core_common::post_login_check::PostLoginValidator;

mod complete;
mod want_create;
mod want_password;
mod want_recovery;
mod want_username;

type StateResult<E = SignupError> = Result<State, E>;

/// Holds the chosen username type.
#[derive(Debug, Clone)]
pub enum Username {
    Internal { username: String, domain: String },
    External { email: String },
}

/// Recovery info: an email or phone number.
#[derive(Debug, Clone)]
pub enum Recovery {
    Email(String),
    Phone(String),
    None,
}

#[derive(Debug, Display, Clone, From, TryInto)]
pub enum State {
    WantUsername(WantUsername),
    WantPassword(WantPassword),
    WantRecovery(WantRecovery),
    WantCreate(WantCreate),
    Complete(Complete),
}

impl State {
    /// Creates a new signup flow state, starting at `WantUsername`.
    #[must_use]
    pub fn new(client: Client, challenge_info: ChallengeInfo) -> Self {
        info!("Signup flow starts");

        let data = StateData { challenge_info };

        WantUsername::new(client, data).into()
    }

    #[must_use]
    pub fn kind(&self) -> StateKind {
        StateKind::of(self)
    }

    pub async fn submit_username(
        self,
        username: Username,
        behavior: Option<Behavior>,
    ) -> StateResult {
        let s: WantUsername = self.try_into().map_err(|_| SignupError::InvalidState)?;

        s.submit_username(username, behavior).await
    }

    pub async fn submit_recovery(
        self,
        recovery: Recovery,
        recovery_behavior: Option<Behavior>,
    ) -> StateResult {
        let s: WantRecovery = self.try_into().map_err(|_| SignupError::InvalidState)?;

        s.submit_recovery(recovery, recovery_behavior).await
    }

    pub fn submit_password(self, password: SecureString) -> StateResult {
        let s: WantPassword = self.try_into().map_err(|_| SignupError::InvalidState)?;

        Ok(s.submit_password(password))
    }

    pub async fn create(
        self,
        store: DynStore,
        post_login_validator: &dyn PostLoginValidator,
    ) -> StateResult {
        let s: WantCreate = self.try_into().map_err(|_| SignupError::InvalidState)?;

        s.create(store, post_login_validator).await
    }

    pub fn complete(self) -> Result<(Client, User, Address), SignupError> {
        let s: Complete = self.try_into().map_err(|_| SignupError::InvalidState)?;

        Ok(s.into_inner())
    }
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
    WantUsername,
    WantPassword,
    WantRecovery,
    WantCreate,
    Complete,
}

impl StateKind {
    fn of(state: &State) -> Self {
        match state {
            State::WantUsername(_) => Self::WantUsername,
            State::WantPassword(_) => Self::WantPassword,
            State::WantRecovery(_) => Self::WantRecovery,
            State::WantCreate(_) => Self::WantCreate,
            State::Complete(_) => Self::Complete,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateData {
    pub challenge_info: ChallengeInfo,
}
