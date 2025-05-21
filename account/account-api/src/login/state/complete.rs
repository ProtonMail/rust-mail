use proton_core_api::{
    services::proton::{SessionId, UserId},
    session::Session,
};

use crate::login::state::{HasSessionId, HasUserId, StateData};

/// Represents a completed login flow.
pub struct Complete {
    client: muon::Client,
    data: StateData,
}

impl Complete {
    pub fn new(client: muon::Client, data: StateData) -> Self {
        Self { client, data }
    }

    pub fn into_session(self) -> Session {
        Session::from_parts(self.client, self.data.parts)
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
