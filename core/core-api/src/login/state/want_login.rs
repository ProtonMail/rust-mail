use std::sync::Arc;

use crate::login::state::StateData;
use crate::login::{state::State, LoginError};
use crate::services::proton::prelude::{AuthId, UserId};
use crate::session::Config;
use crate::store::{AuthInfo, DynStore, MbpMode, TfaMode};
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginExtraInfo, LoginFlow, LoginFlowData};
use muon::client::PasswordMode::{One, Two};
use tracing::info;

/// Represents the initial state of the login flow;
/// the user must call `login` to proceed.
pub struct WantLogin {
    flow: AuthFlow,
    config: Arc<Config>,
    store: DynStore,
}

impl WantLogin {
    pub fn new(flow: AuthFlow, config: Arc<Config>, store: DynStore) -> Self {
        info!("Login flow wants login");

        Self {
            flow,
            config,
            store,
        }
    }

    pub async fn login(
        self,
        user: String,
        pass: String,
        extra_info: LoginExtraInfo,
    ) -> Result<State, (State, LoginError)> {
        self.store.write().await.set_name_or_addr(&user);

        self.try_login(user, pass, extra_info)
            .map_err(|err| (State::LoginError, err))
            .await
    }

    async fn try_login(
        self,
        user: String,
        pass: String,
        extra_info: LoginExtraInfo,
    ) -> Result<State, LoginError> {
        match self.flow.login_with_extra(&user, &pass, extra_info).await {
            LoginFlow::Ok(client, flow_data) => {
                info!("Login flow does not require 2FA");

                let info = get_auth_info(&flow_data, false, false);
                self.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.config, self.store);

                match flow_data.password_mode {
                    One => State::finalize(client, data, pass).await,
                    Two => Ok(State::want_mbp(client, data)),
                }
            }

            LoginFlow::TwoFactor(flow, flow_data) => {
                info!("Login flow requires 2FA");

                let info = get_auth_info(&flow_data, flow.has_totp(), flow.has_fido());
                self.store.write().await.set_auth_info(info).await?;
                let data = get_state_data(&flow_data, self.config, self.store);

                match flow_data.password_mode {
                    One => Ok(State::want_tfa(flow.into(), data, Some(pass))),
                    Two => Ok(State::want_tfa(flow.into(), data, None)),
                }
            }

            LoginFlow::Failed { reason, .. } => {
                Err(LoginError::FlowLogin(muon::Error::from(reason).into()))
            }
        }
    }
}

fn get_auth_info(data: &LoginFlowData, totp: bool, fido: bool) -> AuthInfo {
    AuthInfo {
        user_id: UserId::from(data.user_id.clone()),
        session_id: AuthId::from(data.session_id.clone()),
        tfa_mode: TfaMode::new(totp, fido),
        mbp_mode: MbpMode::from(data.password_mode),
    }
}

fn get_state_data(data: &LoginFlowData, config: Arc<Config>, store: DynStore) -> StateData {
    StateData {
        config,
        store,
        user_id: UserId::from(data.user_id.clone()),
        auth_id: AuthId::from(data.session_id.clone()),
    }
}
