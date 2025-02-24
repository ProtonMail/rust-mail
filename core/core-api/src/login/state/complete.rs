use crate::login::state::{HasAuthId, HasUserId, StateData};
use crate::services::proton::common::{AuthId, UserId};
use crate::services::proton::Proton;
use crate::session::{Session, SessionParts};

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
        Session::from_parts(SessionParts {
            client: self.client,
            config: self.data.config,
            store: self.data.store,
            status: self.data.status,
        })
    }
}

impl HasUserId for Complete {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasAuthId for Complete {
    fn auth_id(&self) -> &AuthId {
        &self.data.auth_id
    }
}
