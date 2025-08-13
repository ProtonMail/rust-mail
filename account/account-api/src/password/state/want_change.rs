use super::{State, StateData};
use crate::password::PasswordError;
use crate::password::state::complete::Complete;
use crate::shared::SecureString;
use crate::{AccountApi, prelude::*};
use derive_more::{Deref, DerefMut};
use futures::TryFutureExt;
use muon::Client;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::prelude::*;
use proton_core_common::datatypes::PasswordMode;
use proton_crypto_account::proton_crypto;
use proton_crypto_account::proton_crypto::crypto::{DataEncoding::Armor, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::KeySalt;

/// Represents the password change flow state where we're waiting for the new password.
#[derive(Clone, Deref, DerefMut)]
pub struct WantChange {
    data: StateData,
}

impl WantChange {
    pub(crate) fn new(data: StateData) -> Self {
        Self { data }
    }

    pub async fn change_pass(self, new_pass: SecureString) -> Result<State, PasswordError> {
        let Self { data } = self;

        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        match data.mbp_mode {
            PasswordMode::One => {
                let auth = get_auth_input(&srp, &data.client, &new_pass).await?;
                change_private_key_password(&srp, &pgp, &data, &new_pass, Some(auth)).await?;
            }

            PasswordMode::Two => {
                change_settings_password(&srp, &data.client, &new_pass).await?;
            }
        }

        Ok(State::Complete(Complete::new(data.client, data.parts)))
    }

    pub async fn change_mbox_pass(
        self,
        new_mbox_pass: SecureString,
    ) -> Result<State, PasswordError> {
        let Self { data } = self;

        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        change_private_key_password(&srp, &pgp, &data, &new_mbox_pass, None).await?;

        Ok(State::Complete(Complete::new(data.client, data.parts)))
    }
}

async fn change_settings_password(
    srp: &impl SRPProvider,
    client: &Client,
    pass: &SecureString,
) -> Result<(), PasswordError> {
    let request = PutSettingsPasswordRequest {
        auth: get_auth_input(srp, client, pass).await?,
    };

    client
        .put_settings_password(request)
        .map_err(PasswordError::Api)
        .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn change_private_key_password(
    srp: &impl SRPProvider,
    pgp: &impl PGPProviderSync,
    data: &StateData,
    new_pass: &SecureString,
    auth: Option<AuthInput>,
) -> Result<(), PasswordError> {
    // Unlock the user's key(s)
    let keys = match data.user_keys.unlock(pgp, &data.key_secret) {
        result if !result.unlocked_keys.is_empty() => result.unlocked_keys,
        _ => return Err(PasswordError::KeySecretDecryption),
    };

    // Generate new key pass
    let new_salt = KeySalt::generate();
    let new_key_pass = new_salt.salted_key_passphrase(srp, new_pass.as_bytes())?;

    // Re-encrypt all keys with new passphrase
    let mut new_keys = Vec::new();

    for key in &keys {
        let key_arm = pgp.private_key_export(&key.private_key, &new_key_pass, Armor)?;
        let key_str = String::from_utf8(key_arm.as_ref().to_vec())?;

        new_keys.push(UpdateKeyRequest {
            id: key.id.to_string(),
            private_key: key_str,
        });
    }

    // Update the user's keys on the server
    let request = PutKeysPrivateRequest {
        auth,
        key_salt: new_salt.to_string(),
        user_keys: Some(new_keys),
        keys: None,
    };

    (data.client.put_keys_private(request))
        .map_err(PasswordError::Api)
        .await?;

    // Update the key secret in the store
    (data.parts.store.write().await)
        .set_key_secret(UserKeySecret(new_key_pass))
        .await?;

    Ok(())
}

async fn get_auth_input(
    srp: &impl SRPProvider,
    client: &Client,
    pass: &SecureString,
) -> Result<AuthInput, PasswordError> {
    let response = client
        .get_auth_modulus()
        .map_err(PasswordError::Api)
        .await?;

    let verifier = srp.generate_client_verifier(pass.as_str(), &response.modulus)?;

    Ok(AuthInput {
        modulus_id: response.modulus_id,
        version: verifier.version,
        salt: verifier.salt,
        verifier: verifier.verifier,
    })
}
