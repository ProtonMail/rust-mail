use futures::TryFutureExt;
use proton_core_api::services::proton::{SessionId, UserId};
use tracing::info;

use crate::login::PostLoginValidator;
use crate::login::state::{HasSessionId, HasUserId, StateData};
use crate::login::{LoginError, state::State};
use crate::shared::SecureString;

/// Represents the login flow state where the user must provide their mailbox password.
pub struct WantMbp {
    client: muon::Client,
    data: StateData,
}

impl WantMbp {
    pub(crate) fn new(client: muon::Client, data: StateData) -> Self {
        info!("Login flow wants mailbox password");

        Self { client, data }
    }

    pub async fn submit_mbp(
        self,
        pass: SecureString,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, (State, LoginError)> {
        let user_id = self.data.user_id.clone();
        let session_id = self.data.session_id.clone();

        State::finalize(self.client, self.data, pass, post_login_validator)
            .map_err(|err| (State::MbpRetry(user_id, session_id), err))
            .await
    }
}

impl HasUserId for WantMbp {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasSessionId for WantMbp {
    fn session_id(&self) -> &SessionId {
        &self.data.session_id
    }
}
