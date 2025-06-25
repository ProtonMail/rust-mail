use std::borrow::Borrow;

use muon::Client;
use proton_core_api::session::{Session, SessionParts};

/// Represents the completed password change flow state.
pub struct Complete {
    client: Client,
    parts: SessionParts,
}

impl Complete {
    pub(crate) fn new(session: impl Borrow<Session>) -> Self {
        let (client, parts) = session.borrow().to_parts();

        Self { client, parts }
    }

    #[must_use]
    pub fn into_session(self) -> Session {
        Session::from_parts(self.client, self.parts)
    }

    #[must_use]
    pub fn api(&self) -> &Client {
        &self.client
    }
}
