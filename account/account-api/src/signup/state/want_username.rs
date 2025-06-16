use crate::shared::challenge::Behavior;
use crate::signup::SignupError;
use crate::signup::state::want_password::WantPassword;
use crate::signup::state::{StateData, StateResult, Username};
use crate::{AccountApi, requests::ParseDomain};
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;
use tracing::info;

/// Represents the state where the user needs to provide username.
#[derive(Debug, Display, Clone)]
#[display("WantUsername")]
pub struct WantUsername {
    client: Client,
    data: StateData,
}

impl WantUsername {
    pub fn new(client: Client, data: StateData) -> Self {
        info!("Signup flow wants username");

        Self { client, data }
    }

    /// Submits chosen username, confirming availability with `AccountApi::check_username_availability`.
    pub async fn submit_username(
        self,
        username: Username,
        behavior: Option<Behavior>,
    ) -> StateResult {
        info!("Submitting username");

        match username.clone() {
            Username::Internal { username, .. } => {
                self.client
                    .check_username_availability(username, ParseDomain::NoEmail, None)
                    .map_err(|_| SignupError::UsernameUnavailable)
                    .await?;
            }

            Username::External { email } => {
                self.client
                    .check_external_username_availability(email, None)
                    .map_err(|_| SignupError::UsernameUnavailable)
                    .await?;
            }
        }

        let mut data = self.data;
        data.challenge_info.username_behavior = behavior;

        Ok(WantPassword::new(self.client, username, data).into())
    }
}
