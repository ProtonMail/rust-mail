use crate::signup::state::want_recovery::WantRecovery;
use crate::signup::state::{State, Username};
use derive_more::Display;
use muon::Client;
use tracing::info;

/// Represents the state where the user needs to provide password.
#[derive(Debug, Display, Clone)]
#[display("WantPassword")]
pub struct WantPassword {
    client: Client,
    username: Username,
}

impl WantPassword {
    pub fn new(client: Client, username: Username) -> Self {
        info!("Signup flow wants password");

        Self { client, username }
    }

    /// Submits chosen password
    pub fn submit_password(self, password: String) -> State {
        info!("Submitting password");

        // Asking for recovery is a default and always after password
        WantRecovery::new(self.client, self.username, password).into()
    }
}
