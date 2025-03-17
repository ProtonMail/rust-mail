use crate::login::state::{HasSessionId, HasUserId, StateData};
use crate::services::proton::Proton;
use crate::services::proton::{SessionId, UserId};
use crate::session::Session;

/// Represents a completed login flow.
pub struct Complete {
    client: Proton,
    data: StateData,
}

impl Complete {
    pub fn new(client: Proton, data: StateData) -> Self {
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
