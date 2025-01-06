use tracing::info;

use crate::login::state::{HasAuthId, HasUserId, StateData, SubmitMbp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::Proton;

/// Represents the login flow state where the user must provide their mailbox password
/// (resumed from a previous login attempt).
pub struct WantResumeMboxPass {
    client: Proton,
    data: StateData,
}

impl WantResumeMboxPass {
    pub fn new(client: Proton, data: StateData) -> Self {
        info!("Login flow wants to resume from mailbox password");

        Self { client, data }
    }
}

impl HasUserId for WantResumeMboxPass {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for WantResumeMboxPass {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}

impl SubmitMbp for WantResumeMboxPass {
    async fn submit_mbp(self, pass: String) -> Result<State, LoginError> {
        State::finalize(self.client, self.data, pass).await
    }
}
