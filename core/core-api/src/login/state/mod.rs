use crate::login::state::complete::Complete;
use crate::login::state::want_mbp::WantMbp;
use crate::login::state::want_mbp_resume::WantMbpResume;
use crate::login::state::want_tfa::WantTfa;
use crate::login::state::want_tfa_resume::WantTfaResume;
use crate::login::{state::want_login::WantLogin, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::session::Session;
use crate::store::DynStore;
use derive_more::From;
use futures::TryFutureExt;
use muon::client::flow::LoginTwoFactorFlow;
use std::fmt::{Debug, Formatter, Result as FmtResult};

mod complete;
mod want_login;
mod want_mbp;
mod want_mbp_resume;
mod want_tfa;
mod want_tfa_resume;

/// Represents the possible states that the login flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(From)]
pub enum State {
    /// The flow is waiting for the user to provide their login credentials.
    WantLogin(WantLogin),

    /// The flow is waiting for the user to provide a 2FA token.
    WantTfa(WantTfa),

    /// The flow is waiting for the user to provide a 2FA token (resumed).
    WantTfaResume(WantTfaResume),

    /// The flow is waiting for the user to provide their mailbox password.
    WantMbp(WantMbp),

    /// The flow is waiting for the user to provide their mailbox password (resumed).
    WantMbpResume(WantMbpResume),

    /// The flow has been completed.
    Complete(Complete),
}

impl State {
    /// Create a `WantLogin` state.
    pub fn want_login(client: Proton, store: DynStore) -> Self {
        WantLogin::new(client, store).into()
    }

    /// Create a `WantTfa` state.
    pub fn want_tfa(
        flow: LoginTwoFactorFlow,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
        pass: Option<String>,
    ) -> Self {
        WantTfa::new(flow, store, user_id, auth_id, pass).into()
    }

    /// Create a `WantTfaResume` state.
    pub fn want_tfa_resume(
        client: Proton,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
    ) -> Self {
        WantTfaResume::new(client, store, user_id, auth_id).into()
    }

    /// Create a `WantMbp` state.
    pub fn want_mbp(client: Proton, store: DynStore, user_id: RemoteId, auth_id: RemoteId) -> Self {
        WantMbp::new(client, store, user_id, auth_id).into()
    }

    /// Create a `WantMbpResume` state.
    pub fn want_mbp_resume(
        client: Proton,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
    ) -> Self {
        WantMbpResume::new(client, store, user_id, auth_id).into()
    }

    /// Attempt to finalize the login flow, transitioning to the `Complete` state if successful.
    pub async fn finalize(
        client: Proton,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
        pass: String,
    ) -> Result<Self, LoginError> {
        Complete::new(client, store, user_id, auth_id, pass)
            .ok_into()
            .await
    }

    /// Attempt to login with the provided credentials.
    pub async fn login(self, user: String, pass: String) -> Result<Self, LoginError> {
        if let Self::WantLogin(state) = self {
            Ok(state.login(user, pass).await?)
        } else {
            Err(LoginError::InvalidState)
        }
    }

    /// Attempt to submit a TOTP code.
    pub async fn submit_totp(self, code: String) -> Result<Self, LoginError> {
        Ok(match self {
            Self::WantTfa(state) => state.submit_totp(code).await?,
            Self::WantTfaResume(state) => state.submit_totp(code).await?,

            _ => return Err(LoginError::InvalidState),
        })
    }

    /// Attempt to submit a mailbox password.
    pub async fn submit_mbp(self, pass: String) -> Result<Self, LoginError> {
        Ok(match self {
            Self::WantMbp(state) => state.submit_mbp(pass).await?,
            Self::WantMbpResume(state) => state.submit_mbp(pass).await?,

            _ => return Err(LoginError::InvalidState),
        })
    }

    /// Attempt to take the completed session from the flow.
    pub fn into_session(self) -> Result<Session, LoginError> {
        if let Self::Complete(state) = self {
            Ok(state.into_session()?)
        } else {
            Err(LoginError::InvalidState)
        }
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
            Self::WantMbp(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.auth_id())
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
        }
    }
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
trait SubmitTfa {
    async fn submit_totp(self, code: String) -> Result<State, LoginError>;

    #[allow(unused)]
    async fn submit_fido(self, code: String) -> Result<State, LoginError>;
}

/// A trait for states that can accept a mailbox password.
trait SubmitMbp {
    async fn submit_mbp(self, pass: String) -> Result<State, LoginError>;
}
