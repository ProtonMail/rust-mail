use crate::login::PostLoginValidator;
use crate::login::state::StateData;
use crate::login::{LoginError, state::State};
use crate::shared::SecureString;
use crate::shared::challenge::{Behavior, ChallengeInfo, ChallengePayload};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginFlow, LoginFlowData, WithCodeFlow};
use muon::client::{Auth, Tokens};
use muon::rest::auth::v4::fido2;
use proton_core_api::auth::KeySecret;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::observability::metrics::AuthV4RequestMetric;
use proton_core_api::services::observability::{
    ApiServiceObservabilityResponse, ObservabilityRecorder,
};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::SessionParts;
use proton_core_api::store::{AuthInfo, MbpMode, TfaMode, UserData};
use proton_core_api::{metric, services::observability::ObservabilityMetric};
use proton_crypto_account::proton_crypto::generate_secure_random_bytes;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::to_value;
use tracing::info;

use super::want_qr_confirmation::WantQrConfirmation;

#[allow(deprecated)]
use muon::client::flow::LoginExtraInfo;

/// Represents the initial state of the login flow;
/// the user must call `login` to proceed.
pub struct WantLogin {
    client: muon::Client,
    flow: AuthFlow,
    parts: SessionParts,
    observability: ObservabilityRecorder,
    challenge_info: Option<ChallengeInfo>,
}

impl WantLogin {
    pub fn new(
        client: muon::Client,
        parts: SessionParts,
        challenge_info: Option<ChallengeInfo>,
    ) -> Self {
        info!("Login flow wants login");
        let flow = client.clone().auth();
        Self {
            client,
            flow,
            parts,
            observability: ObservabilityRecorder::default(),
            challenge_info,
        }
    }

    #[allow(deprecated)]
    pub async fn login_with_credentials(
        mut self,
        user: String,
        pass: SecureString,
        user_behavior: Option<Behavior>,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, (State, LoginError)> {
        self.parts.store.write().await.set_name_or_addr(&user);

        let info = self
            .challenge_info
            .as_mut()
            .and_then(|ci| {
                ci.username_behavior = user_behavior;
                ChallengePayload::new(ci).and_then(|payload| to_value(payload).ok())
            })
            .map_or_else(LoginExtraInfo::default, |json_value| {
                LoginExtraInfo::builder()
                    .with_fingerprint(json_value.into())
                    .build()
            });

        self.try_login(user, pass, info, post_login_validator)
            .map_err(|err| (State::LoginRetry, err))
            .await
    }

    pub async fn generate_sign_in_qr_code(
        self,
        need_encryption_key: bool,
    ) -> Result<State, LoginError> {
        let flow = match self.client.auth().from_fork().with_code().await {
            WithCodeFlow::Poll(flow) => {
                self.observability.record(QrLoginInitiateFork::success());
                flow
            }
            WithCodeFlow::Ok(_client, _vec) => {
                self.observability.record(QrLoginInitiateFork::unknown());
                error!("Client is in invalid state, the fork must not be complete yet");
                return Err(LoginError::InvalidState);
            }
            WithCodeFlow::Failed { reason, .. } => {
                // `FlowErr` type is somehow not accessable, so cannot match on `reason`, so let's use a
                // generic error variant
                self.observability.record(QrLoginInitiateFork::error());
                error!("Failed to initiate client forking: {reason}");
                return Err(LoginError::InvalidState);
            }
        };

        let encryption_key = if need_encryption_key {
            let encryption_key: [u8; 32] = generate_secure_random_bytes();
            KeySecret::new(encryption_key.to_vec())
        } else {
            KeySecret::new(vec![])
        };

        let qr_code_version = 0;
        let user_code = flow.code().to_owned();
        let encryption_key_base64 = BASE64_STANDARD.encode(encryption_key.as_bytes());
        let client_id = self.parts.config.get_client_id();
        let qr_code = format!("{qr_code_version}:{user_code}:{encryption_key_base64}:{client_id}");
        Ok(State::WantQrConfirmation(WantQrConfirmation {
            user_code,
            qr_code,
            encryption_key,
            parts: self.parts,
            observability: self.observability,
            fork_flow: flow,
        }))
    }

    /// Migrate session created by the legacy version of the app
    ///
    pub async fn migrate(
        self,
        client: muon::Client,
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
        client: muon::Client,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<State, LoginError> {
        self.parts
            .store
            .write()
            .await
            .set_name_or_addr(&user.username);
        let info = get_auth_info(&data, false, None);
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

    #[allow(deprecated)]
    async fn try_login(
        self,
        user: String,
        pass: SecureString,
        info: LoginExtraInfo,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, LoginError> {
        match self.flow.login_with_extra(user, pass.as_str(), info).await {
            LoginFlow::Ok(client, flow_data) => {
                check_store_auth(&self.parts, &flow_data.user_id).await?;

                info!("Login flow does not require 2FA");
                self.observability.record(AuthV4RequestMetric::new(
                    ApiServiceObservabilityResponse::Success,
                ));

                let info = get_auth_info(&flow_data, false, None);
                self.parts.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.parts);

                // Always inspect the user regardless of password mode from auth call
                // The password mode from auth is unreliable for temporary password users
                State::inspect_user(
                    client,
                    data,
                    pass,
                    flow_data.password_mode.into(),
                    post_login_validator,
                )
                .await
            }

            LoginFlow::TwoFactor(flow, flow_data) => {
                check_store_auth(&self.parts, &flow_data.user_id).await?;

                info!("Login flow requires 2FA");
                self.observability.record(AuthV4RequestMetric::new(
                    ApiServiceObservabilityResponse::Success,
                ));

                // Always cache the password temporarily - we'll determine later if we need it
                self.parts.store.write().await.set_temp_pass(&pass).await?;

                let info = get_auth_info(&flow_data, flow.has_totp(), flow.fido_details());
                let mode = flow_data.password_mode.into();
                self.parts.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.parts);
                let fido_details = flow.fido_details().cloned();

                // Always pass the password to TFA state - password mode from auth is unreliable
                Ok(State::want_tfa(flow.into(), data, pass, mode, fido_details))
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

metric! {
    #[name = "core_qr_login_initiate_fork_total"]
    #[version = 1]
    #[doc = "This metric type records the outcomes of the `GET auth/v4/sessions/forks` API call."]
    pub struct QrLoginInitiateFork {
        pub status: ApiServiceObservabilityResponse
    }
}
impl QrLoginInitiateFork {
    fn success() -> Self {
        QrLoginInitiateFork {
            status: ApiServiceObservabilityResponse::Success,
        }
    }
    fn unknown() -> Self {
        QrLoginInitiateFork {
            status: ApiServiceObservabilityResponse::Unknown,
        }
    }
    fn error() -> Self {
        QrLoginInitiateFork {
            status: ApiServiceObservabilityResponse::NetworkError,
        }
    }
}

// Check that the auth was saved by muon to the store.
// Our db has the constraint that each account can have at most one session.
// If a user tries to log in with the same account twice, the second session will be rejected.
// However, muon fails silently if it cannot write to the store (not my fault).
// So here we check that muon actually managed to save the auth.
async fn check_store_auth(parts: &SessionParts, user_id: &str) -> Result<(), LoginError> {
    let lock = parts.store.read().await;

    if let Auth::Internal { .. } = lock.get_auth().await {
        debug!("Session found in store");
        return Ok(());
    }

    if let Some(id) = lock.get_session_id(&UserId::from(user_id)).await? {
        warn!(?id, "Found existing session in database");
        return Err(LoginError::DuplicateSession(id.into_inner()));
    }

    Err(LoginError::MissingSession)
}

fn get_auth_info(
    data: &LoginFlowData,
    totp: bool,
    fido_details: Option<&fido2::Response>,
) -> AuthInfo {
    AuthInfo {
        user_id: UserId::from(data.user_id.clone()),
        session_id: SessionId::from(data.session_id.clone()),
        tfa_mode: TfaMode::new(totp, fido_details.is_some()),
        mbp_mode: MbpMode::from(data.password_mode),
        fido_details: fido_details.cloned(),
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
