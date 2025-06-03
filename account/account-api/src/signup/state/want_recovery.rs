use crate::AccountApi;
use crate::countries::{COUNTRIES, Country};
use crate::prelude::{ValidateEmailRequest, ValidatePhoneRequest};
use crate::signup::SignupError;
use crate::signup::state::want_create::WantCreate;
use crate::signup::state::{Recovery, StateResult, Username};
use derive_more::Display;
use futures::TryFutureExt;
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
    pub async fn submit_recovery(self, recovery: Recovery) -> StateResult {
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

        Ok(WantCreate::new(self.client, self.username, self.password, recovery).into())
    }

    /// Available countries getter.
    #[allow(clippy::unused_self)]
    pub fn available_countries(&self) -> &[Country] {
        COUNTRIES
    }
}
