use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::acquire_password_scope;
use crate::password::state::want_change::WantChange;
use crate::password::state::want_tfa::WantTfa;
use crate::shared::SecureString;
use derive_more::Deref;
use proton_crypto_account::proton_crypto::new_srp_provider;

/// Represents the password change flow state where we're waiting for the current password.
#[derive(Clone, Deref)]
pub struct WantPass {
    data: StateData,
}

impl WantPass {
    #[must_use]
    pub fn new(data: StateData) -> Self {
        Self { data }
    }

    pub async fn submit_pass(self, password: SecureString) -> Result<State, PasswordError> {
        let Self { data } = self;

        if data.tfa_mode.has_tfa() {
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
}
