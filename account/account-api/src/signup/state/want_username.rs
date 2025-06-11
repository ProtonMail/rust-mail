use crate::signup::state::want_password::WantPassword;
use crate::signup::state::{StateResult, Username};
use crate::signup::{ChallengeInfo, SignupError};
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
    challenge_info: ChallengeInfo,
}

impl WantUsername {
    pub fn new(client: Client, challenge_info: ChallengeInfo) -> Self {
        info!("Signup flow wants username");

        Self {
            client,
            challenge_info,
        }
    }

    /// Submits chosen username, confirming availability with `AccountApi::check_username_availability`.
    pub async fn submit_username(self, username: Username) -> StateResult {
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

        Ok(WantPassword::new(self.client, username, self.challenge_info).into())
    }
}
