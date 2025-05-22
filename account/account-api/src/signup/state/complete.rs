use derive_more::Display;

use crate::prelude::*;

/// Represents a completed signup flow, holding the final user details and authenticated client.
/// This state is typically reached after successful key setup (`AccountApi::setup_keys_for_new_account`).
#[derive(Debug, Display, Clone)]
#[display("Complete")]
pub struct Complete {
    client: muon::Client,
    user: User,
    addr: Address,
}

impl Complete {
    pub fn new(client: muon::Client, user: User, addr: Address) -> Self {
        Self { client, user, addr }
    }

    /// Consumes the state and returns the authenticated client and user details.
    pub fn into_inner(self) -> (muon::Client, User, Address) {
        (self.client, self.user, self.addr)
    }

    /// Returns a reference to the client.
    pub fn client(&self) -> &muon::Client {
        &self.client
    }

    /// Returns a reference to the user details.
    pub fn user(&self) -> &User {
        &self.user
    }

    /// Returns a reference to the addresses.
    pub fn addr(&self) -> &Address {
        &self.addr
    }
}
