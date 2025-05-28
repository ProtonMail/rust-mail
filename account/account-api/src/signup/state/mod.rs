use crate::prelude::Address;
use crate::signup::state::want_create::WantCreate;
use crate::signup::state::want_password::WantPassword;
use crate::signup::state::want_recovery::WantRecovery;
use crate::signup::state::want_username::WantUsername;
use crate::{prelude::User, signup::SignupError};
use complete::Complete;
use derive_more::{Display, From, TryInto};
use muon::Client;
use proton_core_api::store::DynStore;

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
    pub fn new(client: Client) -> Self {
        info!("Signup flow starts");

        WantUsername::new(client).into()
    }

    #[must_use]
    pub fn kind(&self) -> StateKind {
        StateKind::of(self)
    }

    pub async fn submit_username(self, username: Username) -> StateResult {
        let s: WantUsername = self.try_into().map_err(|_| SignupError::InvalidState)?;

        s.submit_username(username).await
    }

    pub fn submit_recovery(self, recovery: Recovery) -> StateResult {
        let s: WantRecovery = self.try_into().map_err(|_| SignupError::InvalidState)?;

        Ok(s.submit_recovery(recovery))
    }

    pub fn submit_password(self, password: String) -> StateResult {
        let s: WantPassword = self.try_into().map_err(|_| SignupError::InvalidState)?;

        Ok(s.submit_password(password))
    }

    pub async fn create(self, store: DynStore) -> StateResult {
        let s: WantCreate = self.try_into().map_err(|_| SignupError::InvalidState)?;

        s.create(store).await
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
