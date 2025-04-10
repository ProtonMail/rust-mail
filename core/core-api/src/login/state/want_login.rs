use crate::login::state::StateData;
use crate::login::{LoginError, state::State};
use crate::service::ApiServiceError;
use crate::services::observability::metrics::AuthV4RequestMetric;
use crate::services::observability::{ApiServiceObservabilityResponse, ObservabilityRecorder};
use crate::services::proton::Proton;
use crate::services::proton::{SessionId, UserId};
use crate::session::SessionParts;
use crate::store::{AuthInfo, MbpMode, TfaMode, UserData};
use futures::TryFutureExt;
use muon::client::PasswordMode::{One, Two};
use muon::client::flow::{AuthFlow, LoginExtraInfo, LoginFlow, LoginFlowData};
use muon::client::{Auth, Tokens};
use secrecy::{ExposeSecret, SecretString};
use tracing::info;

/// Represents the initial state of the login flow;
/// the user must call `login` to proceed.
pub struct WantLogin {
    flow: AuthFlow,
    parts: SessionParts,
    observability: ObservabilityRecorder,
}

impl WantLogin {
    pub fn new(flow: AuthFlow, parts: SessionParts) -> Self {
        info!("Login flow wants login");

        Self {
            flow,
            parts,
            observability: ObservabilityRecorder::default(),
        }
    }

    pub async fn login(
        self,
        user: String,
        pass: String,
        info: LoginExtraInfo,
    ) -> Result<State, (State, LoginError)> {
        self.parts.store.write().await.set_name_or_addr(&user);

        self.try_login(user, pass, info)
            .map_err(|err| (State::LoginRetry, err))
            .await
    }

    /// Migrate session created by the legacy version of the app
    ///
    pub async fn migrate(
        self,
        client: Proton,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<State, (State, LoginError)> {
        self.try_migrate(client, user, data, refresh_token)
            .map_err(|err| (State::LoginRetry, err))
            .await
    }

    async fn try_migrate(
        self,
        client: Proton,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<State, LoginError> {
        self.parts
            .store
            .write()
            .await
            .set_name_or_addr(&user.username);
        let info = get_auth_info(&data, false, false);
        self.parts
            .store
            .write()
            .await
            .set_auth(Auth::Internal {
                user_id: info.user_id.clone().to_string(),
                uid: info.session_id.clone().to_string(),
                // By providing an empty access token with an empty scopes list we ensure, that the next time
                // we use the API, we will refresh the token
                // TODO (ET-2454) - use Tokens::refresh() after CoreSession accepts having only refresh token
                tok: Tokens::access("", refresh_token.expose_secret(), Vec::<String>::new()),
            })
            .await?;
        self.parts.store.write().await.set_auth_info(info).await?;
        let data = get_state_data(&data, self.parts);

        State::finalize_migration(client, data, user).await
    }

    async fn try_login(
        self,
        user: String,
        pass: String,
        info: LoginExtraInfo,
    ) -> Result<State, LoginError> {
        match self.flow.login_with_extra(&user, &pass, info).await {
            LoginFlow::Ok(client, flow_data) => {
                info!("Login flow does not require 2FA");
                self.observability.record(AuthV4RequestMetric::new(
                    ApiServiceObservabilityResponse::Success,
                ));

                let info = get_auth_info(&flow_data, false, false);
                self.parts.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.parts);

                match flow_data.password_mode {
                    One => State::finalize(client, data, pass).await,
                    Two => Ok(State::want_mbp(client, data)),
                }
            }

            LoginFlow::TwoFactor(flow, flow_data) => {
                info!("Login flow requires 2FA");
                self.observability.record(AuthV4RequestMetric::new(
                    ApiServiceObservabilityResponse::Success,
                ));

                if let One = flow_data.password_mode {
                    self.parts.store.write().await.set_temp_pass(&pass).await?;
                } else {
                    info!("Not caching password (user has separate mailbox password)");
                }

                let info = get_auth_info(&flow_data, flow.has_totp(), flow.has_fido());
                self.parts.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.parts);

                match flow_data.password_mode {
                    One => Ok(State::want_tfa(flow.into(), data, Some(pass))),
                    Two => Ok(State::want_tfa(flow.into(), data, None)),
                }
            }

            LoginFlow::Failed { reason, .. } => {
                let api_service_err: ApiServiceError = muon::Error::from(reason).into();
                let metric_response: ApiServiceObservabilityResponse = (&api_service_err).into();
                self.observability
                    .record(AuthV4RequestMetric::new(metric_response));
                Err(LoginError::FlowLogin(api_service_err))
            }
        }
    }
}

fn get_auth_info(data: &LoginFlowData, totp: bool, fido: bool) -> AuthInfo {
    AuthInfo {
        user_id: UserId::from(data.user_id.clone()),
        session_id: SessionId::from(data.session_id.clone()),
        tfa_mode: TfaMode::new(totp, fido),
        mbp_mode: MbpMode::from(data.password_mode),
    }
}

fn get_state_data(data: &LoginFlowData, parts: SessionParts) -> StateData {
    StateData {
        parts,
        user_id: UserId::from(data.user_id.clone()),
        session_id: SessionId::from(data.session_id.clone()),
        observability: ObservabilityRecorder::default(),
    }
}
