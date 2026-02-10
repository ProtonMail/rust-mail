use crate::AccountApi;
use crate::countries::{COUNTRIES, Country};
use crate::prelude::{ValidateEmailRequest, ValidatePhoneRequest};
use crate::shared::SecureString;
use crate::shared::challenge::Behavior;
use crate::signup::SignupError;
use crate::signup::state::want_create::WantCreate;
use crate::signup::state::{Recovery, StateData, StateResult, Username};
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;

/// Represents the state where the user can provide recovery information.
#[derive(Debug, Display, Clone)]
#[display("WantRecovery")]
pub struct WantRecovery {
    client: Client,
    username: Username,
    password: SecureString,
    data: StateData,
}

impl WantRecovery {
    pub fn new(
        client: Client,
        username: Username,
        password: SecureString,
        data: StateData,
    ) -> WantRecovery {
        info!("Signup flow wants recovery info");

        Self {
            client,
            username,
            password,
            data,
        }
    }

    /// Submits recovery info, or skips it if `Recovery::None` is provided.
    pub async fn submit_recovery(
        self,
        recovery: Recovery,
        behavior: Option<Behavior>,
    ) -> StateResult {
        match recovery.clone() {
            Recovery::Email(email) => {
                self.client
                    .validate_email(ValidateEmailRequest { email })
                    .map_err(|_| SignupError::RecoveryEmailInvalid)
                    .await?;
            }

            Recovery::Phone(phone) => {
                self.client
                    .validate_phone(ValidatePhoneRequest { phone })
                    .map_err(|_| SignupError::RecoveryPhoneNumberInvalid)
                    .await?;
            }

            Recovery::None => {}
        }

        let mut data = self.data;
        data.challenge_info.recovery_behavior = behavior;

        Ok(WantCreate::new(self.client, self.username, self.password, recovery, data).into())
    }

    /// Available countries getter.
    #[allow(clippy::unused_self)]
    pub fn available_countries(&self) -> &[Country] {
        COUNTRIES
    }
}
