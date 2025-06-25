use super::complete::Complete;
use super::{State, StateData};
use crate::password::PasswordError;
use muon::Client;

/// Represents the password change flow state where we're waiting for the new password.
pub struct WantNewPassword {
    data: StateData,
}

impl WantNewPassword {
    pub(crate) fn new(data: StateData) -> Self {
        Self { data }
    }

    pub async fn submit_new_password(self, pass: String) -> Result<State, PasswordError> {
        let Self { data } = self;

        let session = State::finalize(data, pass).await?;

        Ok(Complete::new(session).into())
    }

    #[must_use]
    pub fn api(&self) -> &Client {
        &self.data.client
    }
}
