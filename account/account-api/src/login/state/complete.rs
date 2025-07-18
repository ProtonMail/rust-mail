use crate::DelinquentState;
use crate::login::state::{HasSessionId, HasUserId, StateData};
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::Session;

/// Represents a completed login flow.
pub struct Complete {
    client: muon::Client,
    data: StateData,
    user: Option<User>,
}

impl Complete {
    pub(crate) fn new(client: muon::Client, data: StateData, user: Option<User>) -> Self {
        Self { client, data, user }
    }

    #[must_use]
    pub fn into_session(self) -> Session {
        Session::from_parts(self.client, self.data.parts)
    }

    #[must_use]
    pub fn delinquent_state(&self) -> Option<DelinquentState> {
        Some(self.user.as_ref()?.delinquent.into())
    }
}

impl HasUserId for Complete {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasSessionId for Complete {
    fn session_id(&self) -> &SessionId {
        &self.data.session_id
    }
}
