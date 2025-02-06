use futures::TryFutureExt;
use tracing::info;

use crate::login::state::{HasAuthId, HasUserId, StateData};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::Proton;

/// Represents the login flow state where the user must provide their mailbox password.
pub struct WantMbp {
    client: Proton,
    data: StateData,
}

impl WantMbp {
    pub fn new(client: Proton, data: StateData) -> Self {
        info!("Login flow wants mailbox password");

        Self { client, data }
    }

    pub async fn submit_mbp(self, pass: String) -> Result<State, (State, LoginError)> {
        let user_id = self.data.user_id.clone();
        let auth_id = self.data.auth_id.clone();

        State::finalize(self.client, self.data, pass)
            .map_err(|err| (State::MbpRetry(user_id, auth_id), err))
            .await
    }
}

impl HasUserId for WantMbp {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for WantMbp {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}
