use crate::countries::{COUNTRIES, Country};
use crate::signup::state::want_create::WantCreate;
use crate::signup::state::{Recovery, State, Username};
use derive_more::Display;
use muon::Client;

/// Represents the state where the user can provide recovery information.
#[derive(Debug, Display, Clone)]
#[display("WantRecovery")]
pub struct WantRecovery {
    client: Client,
    username: Username,
    password: String,
}

impl WantRecovery {
    pub fn new(client: Client, username: Username, password: String) -> WantRecovery {
        info!("Signup flow wants recovery info");

        Self {
            client,
            username,
            password,
        }
    }

    /// Submits recovery info, or skips it if `Recovery::None` is provided.
    pub fn submit_recovery(self, recovery: Recovery) -> State {
        WantCreate::new(self.client, self.username, self.password, recovery).into()
    }

    /// Available countries getter.
    #[allow(clippy::unused_self)]
    pub fn available_countries(&self) -> &[Country] {
        COUNTRIES
    }
}
