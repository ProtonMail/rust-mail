use super::want_new_password::WantNewPassword;
use super::{State, StateData};
use crate::password::PasswordError;
use muon::Client;
use proton_core_api::services::proton::prelude::*;

/// Represents the password change flow state where we're waiting for 2FA authentication.
pub struct WantTfa {
    data: StateData,
}

impl WantTfa {
    pub(crate) fn new(data: StateData) -> Self {
        Self { data }
    }

    pub async fn submit_totp(self, code: String) -> Result<State, PasswordError> {
        let Self { data } = self;

        State::acquire_password_scope(&data, Some(code), None).await?;

        Ok(WantNewPassword::new(data).into())
    }

    pub async fn submit_fido(self, _: String) -> Result<State, PasswordError> {
        let Self { data } = self;

        #[allow(unreachable_code)]
        #[allow(clippy::diverging_sub_expression)]
        State::acquire_password_scope(&data, None, unimplemented!()).await?;

        Ok(WantNewPassword::new(data).into())
    }

    #[must_use]
    pub fn api(&self) -> &Client {
        &self.data.client
    }
}
