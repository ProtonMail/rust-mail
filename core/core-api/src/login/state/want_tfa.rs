use crate::login::state::{HasAuthId, HasUserId, StateData, SubmitFido, SubmitTotp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::{AuthId, UserId};
use muon::client::flow::LoginTwoFactorFlow;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code.
pub struct WantTfa {
    flow: LoginTwoFactorFlow,
    data: StateData,
    pass: Option<String>,
}

impl WantTfa {
    pub fn new(flow: LoginTwoFactorFlow, data: StateData, pass: Option<String>) -> Self {
        info!("Login flow wants 2FA");

        Self { flow, data, pass }
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

impl SubmitTotp for WantTfa {
    async fn submit_totp(self, code: String) -> Result<State, LoginError> {
        let Self { flow, data, pass } = self;

        let client = match flow.totp(&code).await {
            Ok(client) => client,
            Err(err) => return Err(LoginError::FlowTotp(err.into())),
        };

        let state = if let Some(pass) = pass {
            State::finalize(client, data, pass).await?
        } else {
            State::want_mbp(client, data)
        };

        Ok(state)
    }
}

impl SubmitFido for WantTfa {
    async fn submit_fido(self, _: String) -> Result<State, LoginError> {
        unimplemented!()
    }
}
