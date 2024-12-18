use crate::login::state::complete::Complete;
use crate::login::state::want_mbp::WantMboxPass;
use crate::login::state::want_resume_mbp::WantResumeMboxPass;
use crate::login::state::want_resume_tfa::WantResumeTfa;
use crate::login::state::want_tfa::WantTfa;
use crate::login::{state::want_login::WantLogin, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::session::{Config, Session};
use crate::store::DynStore;
use derive_more::From;
use futures::TryFutureExt;
use muon::client::flow::LoginTwoFactorFlow;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;

mod complete;
mod want_login;
mod want_mbp;
mod want_resume_mbp;
mod want_resume_tfa;
mod want_tfa;

/// Represents the possible states that the login flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(From)]
pub enum State {
    /// The flow is waiting for the user to provide their login credentials.
    WantLogin(WantLogin),

    /// The flow is waiting for the user to provide a 2FA token.
    WantTfa(WantTfa),

    /// The flow is waiting for the user to provide a 2FA token (resumed).
    WantTfaResume(WantResumeTfa),

    /// The flow is waiting for the user to provide their mailbox password.
    WantMbp(WantMboxPass),

    /// The flow is waiting for the user to provide their mailbox password (resumed).
    WantMbpResume(WantResumeMboxPass),

    /// The flow has been completed.
    Complete(Complete),

    /// Invalid state, cannot be used.
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    /// Attempt to login with the provided credentials.
    pub async fn login(self, user: String, pass: String) -> Result<Self, LoginError> {
        let state = match self {
            Self::WantLogin(state) => state.login(user, pass).await?,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state)
    }

    /// Attempt to submit a TOTP code.
    pub async fn submit_totp(self, code: String) -> Result<Self, LoginError> {
        let state = match self {
            Self::WantTfa(state) => state.submit_totp(code).await?,
            Self::WantTfaResume(state) => state.submit_totp(code).await?,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state)
    }

    /// Attempt to submit a FIDO code.
    #[allow(unused)]
    pub async fn submit_fido(self, code: String) -> Result<Self, LoginError> {
        let state = match self {
            Self::WantTfa(state) => state.submit_fido(code).await?,
            Self::WantTfaResume(state) => state.submit_fido(code).await?,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state)
    }

    /// Attempt to submit a mailbox password.
    pub async fn submit_mbp(self, pass: String) -> Result<Self, LoginError> {
        let state = match self {
            Self::WantMbp(state) => state.submit_mbp(pass).await?,
            Self::WantMbpResume(state) => state.submit_mbp(pass).await?,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state)
    }

    /// Attempt to take the completed session from the flow.
    pub fn into_session(self) -> Result<Session, LoginError> {
        let session = match self {
            Self::Complete(state) => state.into_session(),
            _ => return Err(LoginError::InvalidState),
        };

        Ok(session)
    }

    /// Get the user ID of the user that has (or is in the process of) logging in.
    pub fn user_id(&self) -> Result<&RemoteId, LoginError> {
        let state: &dyn HasUserId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.user_id())
    }

    /// Get the session ID that has been (or is in the process of) being created.
    pub fn auth_id(&self) -> Result<&RemoteId, LoginError> {
        let state: &dyn HasAuthId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.auth_id())
    }
}

/// Public entrypoints for creating new states.
impl State {
    /// Create a `WantLogin` state.
    pub fn want_login(client: Proton, config: Arc<Config>, store: DynStore) -> Self {
        WantLogin::new(client, config, store).into()
    }

    /// Create a `WantResumeTfa` state.
    pub fn want_resume_tfa(
        client: Proton,
        config: Arc<Config>,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
    ) -> Self {
        let data = StateData {
            config,
            store,
            user_id,
            auth_id,
        };

        WantResumeTfa::new(client, data).into()
    }

    /// Create a `WantResumeMboxPass` state.
    pub fn want_resume_mbp(
        client: Proton,
        config: Arc<Config>,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
    ) -> Self {
        let data = StateData {
            config,
            store,
            user_id,
            auth_id,
        };

        WantResumeMboxPass::new(client, data).into()
    }
}

/// Private entrypoints for creating new states.
impl State {
    /// Create a `WantTfa` state.
    fn want_tfa(flow: LoginTwoFactorFlow, data: StateData, pass: Option<String>) -> Self {
        WantTfa::new(flow, data, pass).into()
    }

    /// Create a `WantMbp` state.
    fn want_mbp(client: Proton, data: StateData) -> Self {
        WantMboxPass::new(client, data).into()
    }

    /// Attempt to finalize the login flow, transitioning to the `Complete` state if successful.
    async fn finalize(client: Proton, data: StateData, pass: String) -> Result<Self, LoginError> {
        Complete::new(client, data, pass).ok_into().await
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::WantLogin(_) => write!(f, "WantLogin"),
            Self::WantTfa(_) => write!(f, "WantTfa"),
            Self::WantTfaResume(_) => write!(f, "WantTfaResume"),
            Self::WantMbp(_) => write!(f, "WantMbp"),
            Self::WantMbpResume(_) => write!(f, "WantMbpResume"),
            Self::Complete(_) => write!(f, "Complete"),
            Self::Invalid => write!(f, "Invalid"),
        }
    }
}

pub(crate) struct StateData {
    config: Arc<Config>,
    store: DynStore,
    user_id: RemoteId,
    auth_id: RemoteId,
}

/// A trait for states in which the user ID is known.
trait HasUserId {
    fn user_id(&self) -> &RemoteId;
}

/// A trait for states in which the auth ID is known.
trait HasAuthId {
    fn auth_id(&self) -> &RemoteId;
}

/// A trait for states that can accept a 2FA code.
trait SubmitTotp {
    async fn submit_totp(self, code: String) -> Result<State, LoginError>;
}

/// A trait for states that can accept a FIDO code.
#[allow(unused)]
trait SubmitFido {
    async fn submit_fido(self, code: String) -> Result<State, LoginError>;
}

/// A trait for states that can accept a mailbox password.
trait SubmitMbp {
    async fn submit_mbp(self, pass: String) -> Result<State, LoginError>;
}
