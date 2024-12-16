use crate::login::state::{HasAuthId, HasUserId, SubmitFido, SubmitTotp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::RemoteId;
use crate::store::DynStore;
use muon::client::flow::LoginTwoFactorFlow;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code.
pub struct WantTfa {
    flow: LoginTwoFactorFlow,
    store: DynStore,
    user_id: RemoteId,
    auth_id: RemoteId,
    pass: Option<String>,
}

impl WantTfa {
    pub fn new(
        flow: LoginTwoFactorFlow,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
        pass: Option<String>,
    ) -> Self {
        info!(%user_id, %auth_id, "Login flow wants 2FA");

        Self {
            flow,
            store,
            user_id,
            auth_id,
            pass,
        }
    }
}

impl HasUserId for WantTfa {
    fn user_id(&self) -> &RemoteId {
        &self.user_id
    }
}

impl HasAuthId for WantTfa {
    fn auth_id(&self) -> &RemoteId {
        &self.auth_id
    }
}

impl SubmitTotp for WantTfa {
    async fn submit_totp(self, code: String) -> Result<State, LoginError> {
        let client = match self.flow.totp(&code).await {
            Ok(client) => client,
            Err(err) => return Err(LoginError::FlowTotp(err.into())),
        };

        let state = if let Some(pass) = self.pass {
            State::finalize(client, self.store, self.user_id, self.auth_id, pass).await?
        } else {
            State::want_mbp(client, self.store, self.user_id, self.auth_id)
        };

        Ok(state)
    }
}

impl SubmitFido for WantTfa {
    async fn submit_fido(self, _: String) -> Result<State, LoginError> {
        unimplemented!()
    }
}
