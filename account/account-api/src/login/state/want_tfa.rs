use crate::login::state::{HasSessionId, HasUserId, StateData};
use crate::login::{LoginError, state::State};
use crate::shared::SecureString;
use derive_more::From;
use futures::TryFutureExt;
use muon::Client;
use muon::client::flow::{AuthFlow, LoginTwoFactorFlow};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::observability::metrics;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::store::MbpMode;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code.
pub struct WantTfa {
    flow: TfaFlow,
    data: StateData,
    pass: SecureString,
    mode: MbpMode,
}

impl WantTfa {
    pub(crate) fn new(flow: TfaFlow, data: StateData, pass: SecureString, mode: MbpMode) -> Self {
        info!("Login flow wants 2FA");

        Self {
            flow,
            data,
            pass,
            mode,
        }
    }

    pub async fn submit_totp(self, code: String) -> Result<State, (State, LoginError)> {
        let Self {
            flow,
            data,
            pass,
            mode,
        } = self;

        let result = flow.totp(&code).await;

        data.observability
            .record(metrics::SignInSubmitTotpTotal::new(
                result.as_ref().err().into(),
            ));

        match result {
            Ok(client) => {
                Self::advance(client, data, pass, mode)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.session_id, pass, mode),
                LoginError::FlowTotp(err),
            )),
        }
    }

    pub async fn submit_fido(self, code: String) -> Result<State, (State, LoginError)> {
        let Self {
            flow,
            data,
            pass,
            mode,
        } = self;

        let result = flow.fido(&code).await;

        data.observability
            .record(metrics::SignInSubmitFidoTotal::new(
                result.as_ref().err().into(),
            ));

        match result {
            Ok(client) => {
                Self::advance(client, data, pass, mode)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.session_id, pass, mode),
                LoginError::FlowFido(err),
            )),
        }
    }

    async fn advance(
        client: Client,
        data: StateData,
        pass: SecureString,
        mode: MbpMode,
    ) -> Result<State, LoginError> {
        data.parts.store.write().await.clear_temp_pass().await?;

        State::inspect_user(client, data, pass, mode).await
    }
}

impl HasUserId for WantTfa {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasSessionId for WantTfa {
    fn session_id(&self) -> &SessionId {
        &self.data.session_id
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

    #[allow(clippy::unused_async)]
    async fn fido(self, _: &str) -> Result<Client, ApiServiceError> {
        unimplemented!()
    }
}
