use crate::auth::UserKeySecret;
use crate::login::state::HasAuthId;
use crate::login::{state::HasUserId, LoginError};
use crate::services::proton::common::RemoteId;
use crate::services::proton::{Proton, ProtonCore};
use crate::session::{Config, Session, SessionParts};
use crate::store::DynStore;
use derive_more::Into;
use futures::TryFutureExt;
use proton_crypto_account::keys::{LockedKey, UserKeys};
use proton_crypto_account::proton_crypto;
use proton_crypto_account::salts::{Salt, Salts};
use tracing::info;

/// Represents a completed login flow.
pub struct Complete {
    client: Proton,
    config: Config,
    store: DynStore,
    user_id: RemoteId,
    auth_id: RemoteId,
}

impl Complete {
    pub async fn new(
        client: Proton,
        config: Config,
        store: DynStore,
        user_id: RemoteId,
        auth_id: RemoteId,
        pass: String,
    ) -> Result<Self, LoginError> {
        info!(%user_id, %auth_id, "Completing login flow");

        // Initialize the crypto providers.
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        // Fetch user info to trigger HV.
        let user = client
            .get_users()
            .map_ok(|res| res.user)
            .map_err(LoginError::UserFetch)
            .await?;

        // Fetch the user's key salts.
        let salts = client
            .get_keys_salts()
            .map_ok(|res| res.key_salts)
            .map_err(LoginError::KeySecretSaltFetch)
            .await?;

        // Build the salts object.
        let salts = Salts::new(salts.into_iter().map(|salt| Salt {
            id: salt.id.into_inner().into(),
            key_salt: salt.key_salt.map(Into::into),
        }));

        // Derive the key secret to unlock the user keys.
        let secret = if let Some(key) = user.keys.primary() {
            salts
                .salt_for_key(&srp, &key.id, pass.as_bytes())
                .map_err(LoginError::KeySecretDerivation)?
        } else {
            return Err(LoginError::MissingPrimaryKey);
        };

        // Check if the key secret can unlock the user keys.
        let secret = if user.keys.unlock(&pgp, &secret).unlocked_keys.is_empty() {
            return Err(LoginError::KeySecretDecryption);
        } else {
            UserKeySecret(secret)
        };

        // Save the derived user secret in the auth store.
        store.write().await.set_key_secret(secret).await?;

        Ok(Self {
            client,
            config,
            store,
            user_id,
            auth_id,
        })
    }

    pub fn into_session(self) -> Session {
        Session::from_parts(SessionParts {
            client: self.client,
            config: self.config,
            store: self.store,
        })
    }
}

impl HasUserId for Complete {
    fn user_id(&self) -> &RemoteId {
        &self.user_id
    }
}

impl HasAuthId for Complete {
    fn auth_id(&self) -> &RemoteId {
        &self.auth_id
    }
}

trait UserKeysExt {
    fn primary(&self) -> Option<&LockedKey>;
}

impl UserKeysExt for UserKeys {
    fn primary(&self) -> Option<&LockedKey> {
        self.as_ref().iter().find(|&key| key.primary)
    }
}
