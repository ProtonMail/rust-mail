use crate::login::state::{HasAuthId, HasUserId, StateData};
use crate::login::{state::State, LoginError};
use crate::service::ApiServiceError;
use crate::services::proton::common::{AuthId, UserId};
use derive_more::From;
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginTwoFactorFlow};
use muon::Client;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code.
pub struct WantTfa {
    flow: TfaFlow,
    data: StateData,
    pass: Option<String>,
}

impl WantTfa {
    pub fn new(flow: TfaFlow, data: StateData, pass: Option<String>) -> Self {
        info!("Login flow wants 2FA");

        Self { flow, data, pass }
    }

    pub async fn submit_totp(self, code: String) -> Result<State, (State, LoginError)> {
        let Self { flow, data, pass } = self;

        match flow.totp(&code).await {
            Ok(client) => {
                Self::advance(client, data, pass)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.auth_id, pass),
                LoginError::FlowTotp(err),
            )),
        }
    }

    pub async fn submit_fido(self, code: String) -> Result<State, (State, LoginError)> {
        let Self { flow, data, pass } = self;

        match flow.fido(&code).await {
            Ok(client) => {
                Self::advance(client, data, pass)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.auth_id, pass),
                LoginError::FlowTotp(err),
            )),
        }
    }

    async fn advance(
        client: Client,
        data: StateData,
        pass: Option<String>,
    ) -> Result<State, LoginError> {
        data.parts.store.write().await.clear_temp_pass().await?;

        let state = if let Some(pass) = pass {
            State::finalize(client, data, pass).await?
        } else {
            State::want_mbp(client, data)
        };

        Ok(state)
    }
}

impl HasUserId for WantTfa {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for WantTfa {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}

#[derive(From)]
pub enum TfaFlow {
    Auth(AuthFlow),
    Login(LoginTwoFactorFlow),
}

impl TfaFlow {
    async fn totp(self, code: &str) -> Result<Client, ApiServiceError> {
        match self {
            Self::Auth(flow) => flow.from_totp(code).err_into().await,
            Self::Login(flow) => flow.totp(code).err_into().await,
        }
    }

    async fn fido(self, code: &str) -> Result<Client, ApiServiceError> {
        match self {
            Self::Auth(flow) => flow.from_fido(code).err_into().await,
            Self::Login(flow) => flow.fido(code).err_into().await,
        }
    }
}
