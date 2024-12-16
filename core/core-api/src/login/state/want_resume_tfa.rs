use crate::login::state::{HasAuthId, HasUserId, SubmitFido, SubmitTotp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::store::DynStore;
use tracing::info;

/// Represents the login flow state where the user must provide their two-factor authentication code
/// (resumed from a previous login attempt).
pub struct WantResumeTfa {
    client: Proton,
    store: DynStore,
    user_id: RemoteId,
    auth_id: RemoteId,
}

impl WantResumeTfa {
    pub fn new(client: Proton, store: DynStore, user_id: RemoteId, auth_id: RemoteId) -> Self {
        info!(%user_id, %auth_id, "Login flow wants to resume from 2FA");

        Self {
            client,
            store,
            user_id,
            auth_id,
        }
    }
}

impl HasUserId for WantResumeTfa {
    fn user_id(&self) -> &RemoteId {
        &self.user_id
    }
}

impl HasAuthId for WantResumeTfa {
    fn auth_id(&self) -> &RemoteId {
        &self.auth_id
    }
}

impl SubmitTotp for WantResumeTfa {
    async fn submit_totp(self, code: String) -> Result<State, LoginError> {
        let client = match self.client.auth().from_totp(code).await {
            Ok(client) => client,
            Err(err) => return Err(LoginError::FlowTotp(err.into())),
        };

        Ok(State::want_mbp(
            client,
            self.store,
            self.user_id,
            self.auth_id,
        ))
    }
}

impl SubmitFido for WantResumeTfa {
    async fn submit_fido(self, _: String) -> Result<State, LoginError> {
        unimplemented!()
    }
}
