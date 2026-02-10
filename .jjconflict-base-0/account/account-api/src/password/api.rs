use crate::AccountApi as _;
use crate::password::PasswordError;

use crate::password::state::{State, StateData};
use crate::prelude::{
    AuthInput, PutKeysPrivateRequest, PutSettingsPasswordRequest, PutUsersPasswordRequest,
    PutUsersPasswordResponse, UpdateKeyRequest,
};
use crate::shared::SecureString;
use crate::shared::challenge::get_auth_info;
use futures::TryFutureExt as _;
use muon::Client;
use muon::rest::auth::v4::fido2;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::PostAuthInfoResponse;
use proton_core_common::datatypes::PasswordMode;
use proton_crypto_account::proton_crypto::crypto::{DataEncoding::Armor, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::proton_crypto::{self};
use proton_crypto_account::salts::KeySalt;

/// A type to ensure that change pasword functionalities are called within an increased privilige scope.
pub struct PasswordScope {
    pub response: PutUsersPasswordResponse,
}

impl PasswordScope {
    /// Temporarily grants your session extra privileges.
    pub async fn acquire(
        srp: &impl SRPProvider,
        client: &Client,
        username: &str,
        password: &SecureString,
        auth_info: Option<PostAuthInfoResponse>,
        two_factor_code: Option<String>,
        fido2: Option<fido2::Request>,
    ) -> Result<PasswordScope, PasswordError> {
        let auth_info = match (auth_info, fido2.is_some()) {
            (Some(info), _) => info,
            (None, true) => return Err(PasswordError::InvalidState),
            (None, false) => {
                get_auth_info(client, username)
                    .map_err(PasswordError::ApiService)
                    .await?
            }
        };

        let client_proof = srp.generate_client_proof(
            username,
            password,
            auth_info.version,
            &auth_info.salt,
            &auth_info.modulus,
            &auth_info.server_ephemeral,
        )?;

        let request = PutUsersPasswordRequest {
            client_ephemeral: client_proof.ephemeral,
            client_proof: client_proof.proof,
            srp_session: auth_info.session.clone(),
            two_factor_code,
            fido2,
        };

        let response = client.put_users_password(request).await?;

        if response.server_proof == client_proof.expected_server_proof {
            Ok(PasswordScope { response })
        } else {
            Err(PasswordError::ServerProof)
        }
    }

    pub async fn change_pass(self, data: &StateData) -> Result<State, PasswordError> {
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        match data.mbp_mode {
            PasswordMode::One => {
                let auth = get_auth_input(&srp, &data.client, &data.new_password).await?;
                change_private_key_password(&srp, &pgp, data, Some(auth)).await?;
            }

            PasswordMode::Two => {
                change_settings_password(&srp, &data.client, &data.new_password).await?;
            }
        }
        Ok(State::Complete)
    }

    pub async fn change_mbox_pass(self, data: &StateData) -> Result<State, PasswordError> {
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        change_private_key_password(&srp, &pgp, data, None).await?;

        Ok(State::Complete)
    }
}

#[allow(clippy::too_many_arguments)]
async fn change_private_key_password(
    srp: &impl SRPProvider,
    pgp: &impl PGPProviderSync,
    data: &StateData,
    auth: Option<AuthInput>,
) -> Result<(), PasswordError> {
    // Unlock the user's key(s)
    let keys = match data.user_keys.unlock(pgp, &data.key_secret) {
        result if !result.unlocked_keys.is_empty() => result.unlocked_keys,
        _ => return Err(PasswordError::KeySecretDecryption),
    };

    // Generate new key pass
    let new_salt = KeySalt::generate();
    let new_key_pass = new_salt.salted_key_passphrase(srp, data.new_password.as_bytes())?;

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
