use crate::login::state::StateData;
use crate::login::{state::State, LoginError};
use crate::services::proton::prelude::{SessionId, UserId};
use crate::services::proton::Proton;
use crate::session::SessionParts;
use crate::store::{AuthInfo, MbpMode, TfaMode, UserData};
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginExtraInfo, LoginFlow, LoginFlowData};
use muon::client::PasswordMode::{One, Two};
use muon::client::{Auth, Tokens};
use tracing::info;

/// Represents the initial state of the login flow;
/// the user must call `login` to proceed.
pub struct WantLogin {
    flow: AuthFlow,
    parts: SessionParts,
}

impl WantLogin {
    pub fn new(flow: AuthFlow, parts: SessionParts) -> Self {
        info!("Login flow wants login");

        Self { flow, parts }
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
        tokens: Tokens,
    ) -> Result<State, (State, LoginError)> {
        self.try_migrate(client, user, data, tokens)
            .map_err(|err| (State::LoginRetry, err))
            .await
    }

    async fn try_migrate(
        self,
        client: Proton,
        user: UserData,
        data: LoginFlowData,
        tokens: Tokens,
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
                tok: tokens,
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

                let info = get_auth_info(&flow_data, flow.has_totp(), flow.has_fido());
                self.parts.store.write().await.set_auth_info(info).await?;
                self.parts.store.write().await.set_temp_pass(&pass).await?;
                let data = get_state_data(&flow_data, self.parts);

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
    }
}
