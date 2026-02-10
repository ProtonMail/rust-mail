use crate::login::PostLoginValidator;
use crate::login::state::{HasSessionId, HasUserId, StateData};
use crate::login::{LoginError, state::State};
use crate::requests::{AddressKeyInput, AsyncUserInitialization, SetupKeysRequest};
use crate::shared::SecureString;
use crate::shared::crypto::{NewAddrKey, NewUserKey};
use crate::{AccountApi, prelude::*};
use futures::TryFutureExt;
use proton_core_api::services::proton::{AddressId, ProtonCore, SessionId, UserId};
use proton_crypto_account::proton_crypto;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use std::collections::HashMap;
use tracing::info;

/// Represents the login flow state where the user must provide a new password
/// (for users with temporary passwords).
pub struct WantNewPassword {
    client: muon::Client,
    data: StateData,
}

impl WantNewPassword {
    pub(crate) fn new(client: muon::Client, data: StateData) -> Self {
        info!("Login flow wants new password for temporary password user");

        Self { client, data }
    }

    pub async fn submit_new_password(
        self,
        new_pass: SecureString,
        post_login_validator: &dyn PostLoginValidator,
    ) -> Result<State, (State, LoginError)> {
        // Initialize crypto providers
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        // Fetch user addresses
        let addr = ProtonCore::get_addresses(&self.client)
            .map_ok(|res| res.addresses)
            .map_err(|e| (State::Invalid, LoginError::AddressFetch(e)))
            .await?;

        // Generate new user key with the new password
        let user_key = NewUserKey::init(&srp, &pgp, new_pass.as_str())
            .map_err(|e| (State::Invalid, LoginError::NewPasswordSetup(e.to_string())))?;

        // Generate address keys for all addresses
        let addr_keys: HashMap<AddressId, NewAddrKey> = addr
            .iter()
            .map(|addr| {
                let addr_key = user_key
                    .init_addr(&pgp, &addr.email)
                    .map_err(|e| LoginError::NewPasswordSetup(e.to_string()))?;
                Ok((addr.id.clone(), addr_key))
            })
            .collect::<Result<_, LoginError>>()
            .map_err(|e| (State::Invalid, e))?;

        // Get auth input for server requests
        let auth = self
            .get_auth_input(&srp, &new_pass)
            .await
            .map_err(|e| (State::Invalid, e))?;

        // Setup keys on server
        self.setup_keys(&auth, &user_key, &addr_keys)
            .await
            .map_err(|e| (State::Invalid, e))?;

        // Re-fetch user to get updated key information
        let user = self
            .client
            .get_users()
            .map_ok(|res| res.user)
            .map_err(|e| (State::Invalid, LoginError::UserFetch(e)))
            .await?;

        // Update the temporary password flag in the store.
        (self.data.parts.store.write().await)
            .set_temp_pass(user.flags.has_temporary_password)
            .map_err(|e| (State::Invalid, LoginError::AuthStore(e)))
            .await?;

        // Call finalize to complete the login process with the new password
        State::finalize(self.client, self.data, new_pass, post_login_validator)
            .await
            .map_err(|e| (State::Invalid, e))
    }

    async fn get_auth_input(
        &self,
        srp: &impl SRPProvider,
        password: &SecureString,
    ) -> Result<AuthInput, LoginError> {
        let response = self
            .client
            .get_auth_modulus()
            .await
            .map_err(|e| LoginError::NewPasswordSetup(e.to_string()))?;

        let verifier = srp
            .generate_client_verifier(password.as_str(), &response.modulus)
            .map_err(|e| LoginError::NewPasswordSetup(e.to_string()))?;

        Ok(AuthInput {
            version: verifier.version,
            modulus_id: response.modulus_id,
            salt: verifier.salt,
            verifier: verifier.verifier,
        })
    }

    async fn setup_keys(
        &self,
        auth: &AuthInput,
        user_key: &NewUserKey,
        addr_keys: &HashMap<AddressId, NewAddrKey>,
    ) -> Result<(), LoginError> {
        let address_keys = addr_keys
            .iter()
            .map(|(id, key)| AddressKeyInput::new(id.as_str(), &key.key, &key.skl))
            .collect();

        let request = SetupKeysRequest {
            auth: auth.clone(),
            primary_key: user_key.key.private_key.to_string(),
            key_salt: user_key.salt.to_string(),
            address_keys,
            encrypted_secret: None,
            org_primary_user_key: None,
            org_activation_token: None,
        };

        self.client
            .setup_keys(AsyncUserInitialization::CalledByClient, request)
            .await
            .map_err(|e| LoginError::NewPasswordSetup(e.to_string()))?;

        Ok(())
    }
}

impl HasUserId for WantNewPassword {
    fn user_id(&self) -> &UserId {
        &self.data.user_id
    }
}

impl HasSessionId for WantNewPassword {
    fn session_id(&self) -> &SessionId {
        &self.data.session_id
    }
}
