use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::acquire_password_scope;
use crate::password::state::want_change::WantChange;
use crate::password::state::want_tfa::WantTfa;
use muon::Client;
use proton_core_common::datatypes::PasswordMode;
use proton_crypto_account::proton_crypto::new_srp_provider;

/// Represents the password change flow state where we're waiting for the current password.
#[derive(Clone)]
pub struct WantPass {
    data: StateData,
}

impl WantPass {
    #[must_use]
    pub fn new(data: StateData) -> Self {
        Self { data }
    }

    pub async fn submit_pass(self, password: String) -> Result<State, PasswordError> {
        let Self { data } = self;

        if data.tfa_mode.want_tfa() {
            return Ok(WantTfa::new(data, password).into());
        }

        acquire_password_scope(
            &new_srp_provider(),
            &data.client,
            &data.username,
            &password,
            None,
            None,
        )
        .await?;

        Ok(WantChange::new(data).into())
    }

    #[must_use]
    pub fn mbp_mode(&self) -> PasswordMode {
        self.data.mbp_mode
    }

    #[must_use]
    pub fn api(&self) -> &Client {
        &self.data.client
    }
}
