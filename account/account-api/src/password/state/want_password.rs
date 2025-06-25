use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::want_new_password::WantNewPassword;
use crate::password::state::want_tfa::WantTfa;
use futures::TryFutureExt;
use muon::Client;
use proton_core_api::services::proton::{PostAuthInfoRequest, ProtonAuth, ProtonCore};
use proton_core_api::session::SessionParts;
use proton_core_common::datatypes::TfaStatus;
use proton_crypto_account::proton_crypto;
use proton_crypto_account::proton_crypto::srp::SRPProvider;

/// Represents the password change flow state where we're waiting for the new password.
pub struct WantPassword {
    client: Client,
    parts: SessionParts,
    tfa: TfaStatus,
}

impl WantPassword {
    pub(crate) fn new(client: Client, parts: SessionParts, tfa: TfaStatus) -> Self {
        Self { client, parts, tfa }
    }

    pub async fn submit_password(self, password: String) -> Result<State, PasswordError> {
        let Self { client, parts, tfa } = self;

        let user = client
            .get_users()
            .map_err(PasswordError::ApiService)
            .await?
            .user;

        // Use either the name or primary address as the username.
        let username = user.name.clone().unwrap_or_else(|| user.email.clone());

        // Get auth info for SRP proof generation
        let response = client
            .post_auth_info(PostAuthInfoRequest {
                username: username.clone(),
            })
            .map_err(PasswordError::ApiService)
            .await?;

        // Create SRP proof
        let client_proof = proton_crypto::new_srp_provider().generate_client_proof(
            &username,
            &password,
            response.version,
            &response.salt,
            &response.modulus,
            &response.server_ephemeral,
        )?;

        // Create state data
        let data = StateData {
            client,
            parts,
            user,
            password,
            client_proof,
            srp_session: response.session,
        };

        // If 2FA is required, cannot acquire password scope yet.
        if tfa.want_tfa() {
            return Ok(WantTfa::new(data).into());
        }

        // Acquire password scope for new password.
        State::acquire_password_scope(&data, None, None).await?;

        Ok(WantNewPassword::new(data).into())
    }

    #[must_use]
    pub fn api(&self) -> &Client {
        &self.client
    }
}
