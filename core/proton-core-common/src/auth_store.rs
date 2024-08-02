//! Implementation of the [`AuthStore`](proton-api-core::auth::Store) over the database.

use crate::datatypes::RemoteId;
use crate::db::session::{DecryptedUserSession, EncryptedUserSession, SessionEncryptionKey};
use crate::os::KeyChain;
use futures::future::BoxFuture;
use futures::FutureExt;
use proton_api_core::auth::{Auth, Store, StoreError};
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;
use std::sync::Arc;
use tracing::error;

/// Auth store implementation which records the data in the session database.
pub struct AuthStore {
    stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_id: Option<RemoteId>,
}

impl AuthStore {
    pub fn new(stash: Stash, key_chain: Arc<dyn KeyChain>, user_id: Option<RemoteId>) -> Self {
        Self {
            stash,
            key_chain,
            user_id,
        }
    }

    fn encryption_key(&self) -> Result<SessionEncryptionKey, StoreError> {
        let Some(key) = self.key_chain.get().map_err(|e| -> StoreError {
            error!("Failed to load secret from key chain: {e}");
            format!("Failed to load secret from key chain: {e}").into()
        })?
        else {
            error!("Keychain has no decryption key");
            return Err("Keychain has no decryption key".into());
        };

        SessionEncryptionKey::from_base64(&key).ok_or("Invalid encryption key".into())
    }
}

impl Store for AuthStore {
    fn set(&mut self, auth: Auth) -> BoxFuture<'_, Result<(), StoreError>> {
        async move {
            let decrypted = DecryptedUserSession::from(auth);

            let key = self.encryption_key()?;

            let mut encrypted = decrypted.to_encrypted_session(&key).map_err(|e| {
                error!("Failed to encrypt session data: {e}");
                Box::new(e)
            })?;

            encrypted.stash = Some(self.stash.clone());
            encrypted.save().await.map_err(|e| -> StoreError {
                error!("Failed to save data to the database: {e}");
                e.into()
            })?;

            self.user_id = Some(decrypted.user_id);
            Ok(())
        }
        .boxed()
    }

    fn get(&self) -> BoxFuture<'_, Result<Option<Auth>, StoreError>> {
        async {
            let Some(user_id) = &self.user_id else {
                error!("Can not load auth from store if no User ID is specified");
                return Err("No user id set".into());
            };

            let key = self.encryption_key()?;

            let Some(encrypted) = EncryptedUserSession::find_first(
                "WHERE user_id =? LIMIT 1",
                params![user_id.clone()],
                &self.stash,
            )
            .await
            .map_err(|e| -> StoreError {
                error!("Failed to load encrypted session from the database: {e}");
                e.into()
            })?
            else {
                return Ok(None);
            };

            let decrypted = encrypted
                .to_decrypted_session(&key)
                .map_err(|e| -> StoreError {
                    error!("Failed to decrypted encrypted session data: {e}");
                    e.into()
                })?;

            Ok(Some(decrypted.into()))
        }
        .boxed()
    }

    fn clear(&mut self) -> BoxFuture<'_, Result<(), StoreError>> {
        async {
            if let Some(user_id) = &self.user_id {
                let query = format!(
                    "DELETE FROM {} WHERE user_id = ?",
                    EncryptedUserSession::table_name()
                );
                self.stash
                    .execute(&query, params![user_id.clone()])
                    .await
                    .map_err(|e| -> StoreError {
                        error!("Failed to remove session from database:{e}");
                        e.into()
                    })?;
            }
            self.user_id = None;
            Ok(())
        }
        .boxed()
    }
}
