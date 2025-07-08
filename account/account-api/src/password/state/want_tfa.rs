use super::want_change::WantChange;
use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::acquire_password_scope;
use crate::requests::Fido2AuthData;
use muon::Client;
use proton_core_common::datatypes::PasswordMode;
use proton_crypto_account::proton_crypto::new_srp_provider;

/// Represents the password change flow state where we're waiting for 2FA authentication.
#[derive(Clone)]
pub struct WantTfa {
    data: StateData,
    password: String,
}

impl WantTfa {
    #[must_use]
    pub fn new(data: StateData, password: String) -> Self {
        Self { data, password }
    }

    pub async fn submit_totp(self, code: String) -> Result<State, PasswordError> {
        let Self { data, password } = self;

        acquire_password_scope(
            &new_srp_provider(),
            &data.client,
            &data.username,
            &password,
            Some(code),
            None,
        )
        .await?;

        Ok(WantChange::new(data).into())
    }

    pub async fn submit_fido(self, fido: Fido2AuthData) -> Result<State, PasswordError> {
        let Self { data, password } = self;

        acquire_password_scope(
            &new_srp_provider(),
            &data.client,
            &data.username,
            &password,
            None,
            Some(fido),
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
