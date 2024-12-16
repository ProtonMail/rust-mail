use crate::login::{state::State, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::session::Config;
use crate::store::DynStore;
use muon::client::flow::LoginFlow;
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
            LoginFlow::Ok(client, data) => {
                let user_id = RemoteId::from(data.user_id);
                let auth_id = RemoteId::from(data.session_id);

                match data.password_mode {
                    One => State::finalize(client, config, store, user_id, auth_id, pass).await?,
                    Two => State::want_mbp(client, config, store, user_id, auth_id),
                }
            }

            LoginFlow::TwoFactor(flow, data) => {
                let user_id = RemoteId::from(data.user_id);
                let auth_id = RemoteId::from(data.session_id);

                match data.password_mode {
                    One => State::want_tfa(flow, config, store, user_id, auth_id, Some(pass)),
                    Two => State::want_tfa(flow, config, store, user_id, auth_id, None),
                }
            }

            LoginFlow::Failed { reason, .. } => {
                return Err(LoginError::FlowLogin(muon::Error::from(reason).into()));
            }
        };

        Ok(state)
    }
}
