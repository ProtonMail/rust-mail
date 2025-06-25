use crate::AccountApi;
use crate::password::state::complete::Complete;
use crate::password::state::want_new_password::WantNewPassword;
use crate::password::state::want_password::WantPassword;
use crate::password::state::want_tfa::WantTfa;
use crate::password::{PasswordError, UserKeysExt};
use derive_more::{Debug, From};
use futures::TryFutureExt;
use muon::Client;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::prelude::*;
use proton_core_api::session::{Session, SessionParts};
use proton_core_api::store::UserData;

use proton_core_common::datatypes::TfaStatus;
use proton_crypto_account::proton_crypto;
use proton_crypto_account::proton_crypto::crypto::{DataEncoding::Armor, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::{ClientProof, ClientVerifier, SRPProvider};
use proton_crypto_account::salts::{KeySalt, Salt, Salts};

pub mod complete;
pub mod want_new_password;
pub mod want_password;
pub mod want_tfa;

/// Represents the possible states that the password change flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(Debug, Default, From)]
pub enum State {
    /// The flow is waiting for the user to provide their current password.
    #[debug("WantPassword")]
    WantPassword(WantPassword),

    /// The flow is waiting for the user to provide a 2FA token.
    #[debug("WantTfa")]
    WantTfa(WantTfa),

    /// The flow is waiting for the user to provide their new password.
    #[debug("WantNewPassword")]
    WantNewPassword(WantNewPassword),

    /// The flow is complete.
    #[debug("Complete")]
    Complete(Complete),

    /// Invalid state, cannot be used.
    #[default]
    #[debug("Invalid")]
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    /// Create a new state machine with current password.
    #[must_use]
    pub fn new(client: Client, parts: SessionParts, tfa: TfaStatus) -> Self {
        WantPassword::new(client, parts, tfa).into()
    }

    /// Submit current password for authentication.
    pub async fn submit_password(self, password: String) -> Result<Self, PasswordError> {
        if let Self::WantPassword(state) = self {
            state.submit_password(password).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit TOTP code for 2FA authentication.
    pub async fn submit_totp(self, code: String) -> Result<Self, PasswordError> {
        if let Self::WantTfa(state) = self {
            state.submit_totp(code).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit FIDO2 response for 2FA authentication.
    pub async fn submit_fido(self, response: String) -> Result<Self, PasswordError> {
        if let Self::WantTfa(state) = self {
            state.submit_fido(response).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Submit new password.
    pub async fn submit_new_password(self, new_pass: String) -> Result<Self, PasswordError> {
        if let Self::WantNewPassword(state) = self {
            state.submit_new_password(new_pass).await
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Take the completed session from the flow.
    pub fn into_session(self) -> Result<Session, PasswordError> {
        if let Self::Complete(state) = self {
            Ok(state.into_session())
        } else {
            Err(PasswordError::InvalidState)
        }
    }

    /// Get the API client for external operations.
    #[must_use]
    pub fn api(&self) -> &Client {
        match self {
            Self::WantPassword(state) => state.api(),
            Self::WantTfa(state) => state.api(),
            Self::WantNewPassword(state) => state.api(),
            Self::Complete(state) => state.api(),
            Self::Invalid => panic!("Cannot get API from invalid state"),
        }
    }

    pub(crate) async fn acquire_password_scope(
        data: &StateData,
        totp: Option<String>,
        fido: Option<Fido2AuthData>,
    ) -> Result<PutUsersPasswordResponse, PasswordError> {
        let request = PutUsersPasswordRequest {
            client_ephemeral: data.client_proof.ephemeral.clone(),
            client_proof: data.client_proof.proof.clone(),
            srp_session: data.srp_session.clone(),
            two_factor_code: totp,
            fido2: fido,
            sso_reauth_token: None,
        };

        let response = (data.client)
            .put_users_password(request)
            .map_err(PasswordError::FlowAuth)
            .await?;

        if response.server_proof == data.client_proof.expected_server_proof {
            Ok(response)
        } else {
            Err(PasswordError::ServerProof)
        }
    }

    pub(crate) async fn finalize(data: StateData, pass: String) -> Result<Session, PasswordError> {
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        // Derive the key passphrase
        let salts = get_key_salts(&data).await?;
        let primary_key = (data.user.keys.primary()).ok_or(PasswordError::MissingPrimaryKey)?;
        let key_pass = salts.salt_for_key(&srp, &primary_key.id, data.password.as_bytes())?;

        // Unlock the user's key(s)
        let keys = match data.user.keys.unlock(&pgp, &key_pass) {
            result if !result.unlocked_keys.is_empty() => result.unlocked_keys,
            _ => return Err(PasswordError::KeySecretDecryption),
        };

        // Generate the new SRP verifier
        let (modulus_id, new_verifier) = get_srp_verifier(&srp, &data, &pass).await?;

        // Generate the new salt and key passphrase
        let new_salt = KeySalt::generate();
        let new_key_pass = new_salt.salted_key_passphrase(&srp, pass.as_bytes())?;

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
            key_salt: new_salt.to_string(),
            keys: None,
            user_keys: Some(new_keys),
            auth: AuthInput {
                modulus_id,
                version: new_verifier.version,
                salt: new_verifier.salt,
                verifier: new_verifier.verifier,
            },
        };

        data.client
            .put_keys_private(request)
            .map_err(PasswordError::ApiService)
            .await?;

        // Update user data in store
        (data.parts.store.write().await)
            .set_user_data(UserData {
                username: data.user.name.unwrap_or_default(),
                display_name: data.user.display_name.unwrap_or_default(),
                primary_addr: data.user.email,
                key_secret: UserKeySecret(new_key_pass),
            })
            .await?;

        Ok(Session::from_parts(data.client, data.parts))
    }
}

async fn get_key_salts(data: &StateData) -> Result<Salts, PasswordError> {
    let response = data
        .client
        .get_keys_salts()
        .map_err(PasswordError::KeySecretSaltFetch)
        .await?;

    let salts = Salts::new(response.key_salts.into_iter().map(|salt| Salt {
        id: salt.id.into_inner().into(),
        key_salt: salt.key_salt.map(Into::into),
    }));

    Ok(salts)
}

async fn get_srp_verifier(
    srp: &impl SRPProvider,
    data: &StateData,
    pass: &String,
) -> Result<(String, ClientVerifier), PasswordError> {
    let response = data
        .client
        .get_auth_modulus()
        .map_err(PasswordError::Api)
        .await?;

    Ok((
        response.modulus_id,
        srp.generate_client_verifier(pass, &response.modulus)?,
    ))
}

/// Shared data between states.
pub(crate) struct StateData {
    pub client: Client,
    pub parts: SessionParts,
    pub user: User,
    pub password: String,
    pub client_proof: ClientProof,
    pub srp_session: String,
}
