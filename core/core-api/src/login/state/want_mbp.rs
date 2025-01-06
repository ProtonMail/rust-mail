use tracing::info;

use crate::login::state::{HasAuthId, HasUserId, StateData, SubmitMbp};
use crate::login::{state::State, LoginError};
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::Proton;

/// Represents the login flow state where the user must provide their mailbox password.
pub struct WantMboxPass {
    client: Proton,
    data: StateData,
}

impl WantMboxPass {
    pub fn new(client: Proton, data: StateData) -> Self {
        info!("Login flow wants mailbox password");

        Self { client, data }
    }
}

impl HasUserId for WantMboxPass {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for WantMboxPass {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}

impl SubmitMbp for WantMboxPass {
    async fn submit_mbp(self, pass: String) -> Result<State, LoginError> {
        State::finalize(self.client, self.data, pass).await
    }
}
