use crate::login::state::{HasSessionId, HasUserId, StateData};
use crate::login::{LoginError, state::State};
use crate::shared::SecureString;
use crate::shared::challenge::get_auth_info;
use derive_more::From;
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginTwoFactorFlow};
use muon::common::Sender;
use muon::rest::auth::v4::fido2;
use muon::{Client, ProtonRequest, ProtonResponse};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::observability::metrics;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_common::post_login_check::PostLoginValidator;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code.
pub struct WantTfa {
    flow: TfaFlow,
    data: StateData,
    username: String,
    pass: SecureString,
    fido_details: Option<fido2::Response>,
}

impl WantTfa {
    pub(crate) fn new(
        flow: TfaFlow,
        data: StateData,
        username: String,
        pass: SecureString,
        fido_details: Option<fido2::Response>,
    ) -> Self {
        info!("Login flow wants 2FA");

        Self {
            flow,
            data,
            username,
            pass,
            fido_details,
        }
    }

    pub async fn submit_totp(
        self,
        code: String,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, (State, LoginError)> {
        let Self {
            flow,
            data,
            username,
            pass,
            fido_details: _,
        } = self;

        let result = flow.totp(&code).await;

        data.observability.record(
            metrics::SignInSubmitTotpTotal::new(result.as_ref().err().into()),
            true,
        );

        match result {
            Ok(client) => {
                Self::advance(client, data, pass, post_login_validator)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err @ ApiServiceError::Unauthorized(_, _)) => {
                Err((State::TfaError, LoginError::FlowTotp(err)))
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.session_id, username, pass),
                LoginError::FlowTotp(err),
            )),
        }
    }

    pub async fn submit_fido(
        self,
        fido_request: fido2::Request,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, (State, LoginError)> {
        let Self {
            flow,
            data,
            username,
            pass,
            fido_details: _,
        } = self;

        let result = flow.fido(fido_request).await;

        data.observability.record(
            metrics::SignInSubmitFidoTotal::new(result.as_ref().err().into()),
            true,
        );

        match result {
            Ok(client) => {
                Self::advance(client, data, pass, post_login_validator)
                    .map_err(|err| (State::TfaError, err))
                    .await
            }

            Err(err) => Err((
                State::TfaRetry(data.user_id, data.session_id, username, pass),
                LoginError::FlowFido(err),
            )),
        }
    }

    async fn advance(
        client: Client,
        data: StateData,
        pass: SecureString,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, LoginError> {
        data.parts.store.write().await.clear_pass().await?;

        State::inspect_user(client, data, pass, post_login_validator).await
    }

    pub async fn fido_details(
        &mut self,
        client: &impl Sender<ProtonRequest, ProtonResponse>,
    ) -> Result<Option<fido2::Response>, LoginError> {
        if self.fido_details.is_none() {
            debug!("request new fido details");
            self.fido_details = get_auth_info(client, &self.username)
                .map_ok(|info| info.fido_details())
                .map_err(LoginError::FlowFido)
                .await?;
        } else {
            debug!("return cached fido details");
        }
        Ok(self.fido_details.clone())
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

    async fn fido(self, fido_request: fido2::Request) -> Result<Client, ApiServiceError> {
        match self {
            Self::Auth(flow) => flow.from_fido(fido_request).err_into().await,
            Self::Login(flow) => flow.fido(fido_request).err_into().await,
        }
    }
}
