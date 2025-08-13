use super::want_change::WantChange;
use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::acquire_password_scope;
use crate::shared::SecureString;
use derive_more::{Deref, DerefMut};
use muon::rest::auth::v4::fido2;
use proton_crypto_account::proton_crypto::new_srp_provider;

/// Represents the password change flow state where we're waiting for 2FA authentication.
#[derive(Clone, Deref, DerefMut)]
pub struct WantTfa {
    #[deref]
    #[deref_mut]
    data: StateData,

    password: SecureString,
}

impl WantTfa {
    #[must_use]
    pub fn new(data: StateData, password: SecureString) -> Self {
        Self { data, password }
    }

    pub async fn submit_totp(self, code: String) -> Result<State, PasswordError> {
        let Self { mut data, password } = self;

        acquire_password_scope(
            &new_srp_provider(),
            &data.client,
            &data.username,
            &password,
            data.auth_info.take(),
            Some(code),
            None,
        )
        .await?;

        Ok(WantChange::new(data).into())
    }

    pub async fn submit_fido(self, fido_data: fido2::Request) -> Result<State, PasswordError> {
        let Self { mut data, password } = self;

        acquire_password_scope(
            &new_srp_provider(),
            &data.client,
            &data.username,
            &password,
            data.auth_info.take(),
            None,
            Some(fido_data),
        )
        .await?;

        Ok(WantChange::new(data).into())
    }
}
