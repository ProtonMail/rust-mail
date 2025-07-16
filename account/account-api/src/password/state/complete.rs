use muon::Client;
use proton_core_api::session::{Session, SessionParts};

/// Represents the completed password change flow state.
#[derive(Clone)]
pub struct Complete {
    client: Client,
    parts: SessionParts,
}

impl Complete {
    pub(crate) fn new(client: Client, parts: SessionParts) -> Self {
        Self { client, parts }
    }

    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    #[must_use]
    pub fn into_session(self) -> Session {
        Session::from_parts(self.client, self.parts)
    }
}
