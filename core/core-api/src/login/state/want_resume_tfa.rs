use crate::login::state::{HasAuthId, HasUserId, StateData, SubmitFido, SubmitTotp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::Proton;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code
/// (resumed from a previous login attempt).
pub struct WantResumeTfa {
    client: Proton,
    data: StateData,
}

impl WantResumeTfa {
    pub fn new(client: Proton, data: StateData) -> Self {
        info!("Login flow wants to resume from 2FA");

        Self { client, data }
    }
}

impl HasUserId for WantResumeTfa {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for WantResumeTfa {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}

impl SubmitTotp for WantResumeTfa {
    async fn submit_totp(self, code: String) -> Result<State, LoginError> {
        match self.client.auth().from_totp(code).await {
            Ok(client) => Ok(State::want_mbp(client, self.data)),
            Err(err) => Err(LoginError::FlowTotp(err.into())),
        }
    }
}

impl SubmitFido for WantResumeTfa {
    async fn submit_fido(self, _: String) -> Result<State, LoginError> {
        unimplemented!()
    }
}
