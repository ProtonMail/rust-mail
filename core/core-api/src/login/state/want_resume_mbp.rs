use tracing::info;

use crate::login::state::{HasAuthId, HasUserId, SubmitMbp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::Proton;
use crate::store::DynStore;

/// Represents the login flow state where the user must provide their mailbox password
/// (resumed from a previous login attempt).
pub struct WantResumeMbp {
    client: Proton,
    store: DynStore,
    user_id: RemoteId,
    auth_id: RemoteId,
}

impl WantResumeMbp {
    pub fn new(client: Proton, store: DynStore, user_id: RemoteId, auth_id: RemoteId) -> Self {
        info!(%user_id, %auth_id, "Login flow wants to resume from mailbox password");

        Self {
            client,
            store,
            user_id,
            auth_id,
        }
    }
}

impl HasUserId for WantResumeMbp {
    fn user_id(&self) -> &RemoteId {
        &self.user_id
    }
}

impl HasAuthId for WantResumeMbp {
    fn auth_id(&self) -> &RemoteId {
        &self.auth_id
    }
}

impl SubmitMbp for WantResumeMbp {
    async fn submit_mbp(self, pass: String) -> Result<State, LoginError> {
        State::finalize(self.client, self.store, self.user_id, self.auth_id, pass).await
    }
}
