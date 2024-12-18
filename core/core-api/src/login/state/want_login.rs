use crate::login::state::StateData;
use crate::login::{state::State, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::session::Config;
use crate::store::{AuthInfo, DynStore, MbpMode, TfaMode};
use muon::client::flow::{LoginFlow, LoginFlowData};
use muon::client::PasswordMode::{One, Two};
use tracing::info;

/// Represents the initial state of the login flow;
/// the user must call `login` to proceed.
pub struct WantLogin {
    client: Proton,
    config: Config,
    store: DynStore,
}

impl WantLogin {
    pub fn new(client: Proton, config: Config, store: DynStore) -> Self {
        info!("Login flow wants login");

        Self {
            client,
            config,
            store,
        }
    }

    pub async fn login(self, user: String, pass: String) -> Result<State, LoginError> {
        let Self {
            client,
            config,
            store,
        } = self;

        store.write().await.set_name_or_addr(&user);

        let state = match client.auth().login(&user, &pass).await {
            LoginFlow::Ok(client, flow_data) => {
                info!("Login flow does not require 2FA");

                let LoginFlowData {
                    user_id,
                    session_id,
                    password_mode,
                } = flow_data;

                let auth_info = AuthInfo {
                    user_id: RemoteId::from(user_id.clone()),
                    session_id: RemoteId::from(session_id.clone()),
                    tfa_mode: TfaMode::none(),
                    mbp_mode: MbpMode::from(password_mode),
                };

                store.write().await.set_auth_info(auth_info).await?;

                let data = StateData {
                    config,
                    store,
                    user_id: RemoteId::from(user_id),
                    auth_id: RemoteId::from(session_id),
                };

                match password_mode {
                    One => State::finalize(client, data, pass).await?,
                    Two => State::want_mbp(client, data),
                }
            }

            LoginFlow::TwoFactor(flow, flow_data) => {
                info!("Login flow requires 2FA");

                let LoginFlowData {
                    user_id,
                    session_id,
                    password_mode,
                } = flow_data;

                let auth_info = AuthInfo {
                    user_id: RemoteId::from(user_id.clone()),
                    session_id: RemoteId::from(session_id.clone()),
                    tfa_mode: TfaMode::new(flow.has_totp(), flow.has_fido()),
                    mbp_mode: MbpMode::from(password_mode),
                };

                store.write().await.set_auth_info(auth_info).await?;

                let data = StateData {
                    config,
                    store,
                    user_id: RemoteId::from(user_id),
                    auth_id: RemoteId::from(session_id),
                };

                match password_mode {
                    One => State::want_tfa(flow, data, Some(pass)),
                    Two => State::want_tfa(flow, data, None),
                }
            }

            LoginFlow::Failed { reason, .. } => {
                return Err(LoginError::FlowLogin(muon::Error::from(reason).into()));
            }
        };

        Ok(state)
    }
}
