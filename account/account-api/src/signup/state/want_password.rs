use crate::shared::SecureString;
use crate::signup::state::want_recovery::WantRecovery;
use crate::signup::state::{State, StateData, Username};
use derive_more::Display;
use muon::Client;
use tracing::info;

/// Represents the state where the user needs to provide password.
#[derive(Debug, Display, Clone)]
#[display("WantPassword")]
pub struct WantPassword {
    client: Client,
    username: Username,
    data: StateData,
}

impl WantPassword {
    pub fn new(client: Client, username: Username, data: StateData) -> Self {
        info!("Signup flow wants password");

        Self {
            client,
            username,
            data,
        }
    }

    /// Submits chosen password
    pub fn submit_password(self, password: SecureString) -> State {
        info!("Submitting password");

        // Asking for recovery is a default and always after password
        WantRecovery::new(self.client, self.username, password, self.data).into()
    }
}
