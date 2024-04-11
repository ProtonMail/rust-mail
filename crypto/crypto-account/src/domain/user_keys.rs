use futures::future::join_all;

use super::{KeyId, LockedKey, UnlockResult};
use crate::{
    errors::{AccountCryptoError, KeyError},
    salts::SaltedPassword,
};
use proton_crypto::crypto::{AsPublicKeyRef, DataEncoding, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

/// Represents locked user keys retrieved from the API.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct UserKeys(pub Vec<LockedKey>);

impl AsRef<[LockedKey]> for UserKeys {
    fn as_ref(&self) -> &[LockedKey] {
        &self.0
    }
}

impl UserKeys {
    pub fn new(v: impl IntoIterator<Item = LockedKey>) -> Self {
        Self(Vec::from_iter(v))
    }

    /// Unlocks/decrypts the locked keys with the provided `salted_password`.
    ///
    /// Returns the user keys that have been successfully decrypted with the
    /// provided password. If decryption fails for a key, the key is ignored.
    pub fn unlock<T: proton_crypto::crypto::PGPProviderSync>(
        &self,
        provider: &T,
        salted_password: &SaltedPassword<impl AsRef<[u8]>>,
    ) -> UnlockResult<DecryptedUserKey<T::PrivateKey, T::PublicKey>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_address_keys: Vec<DecryptedUserKey<_, _>> =
            Vec::with_capacity(self.0.len());
        decrypted_address_keys.extend(self.0.iter().filter_map(|locked_key| {
            let decryption_result = provider.private_key_import(
                &locked_key.private_key,
                salted_password,
                DataEncoding::Armor,
            );
            let private_key = match decryption_result {
                Ok(key) => key,
                Err(err) => {
                    failed_keys.push(KeyError::Unlock(
                        locked_key.id.clone(),
                        AccountCryptoError::KeyImport(err),
                    ));
                    return None;
                }
            };
            let public_key = match provider.private_key_to_public_key(&private_key) {
                Ok(key) => key,
                Err(err) => {
                    failed_keys.push(KeyError::Unlock(
                        locked_key.id.clone(),
                        AccountCryptoError::TransformPublic(err),
                    ));
                    return None;
                }
            };
            Some(DecryptedUserKey {
                private_key,
                public_key,
                id: locked_key.id.clone(),
            })
        }));
        UnlockResult {
            unlocked_keys: decrypted_address_keys,
            failed: failed_keys,
        }
    }

    /// Unlocks/decrypts the locked keys with the `salted_password`.
    ///
    /// Returns the user keys that have been successfully decrypted with the
    /// provided password. If decryption fails, a key is ignored.
    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        salted_password: &SaltedPassword<impl AsRef<[u8]>>,
    ) -> UnlockResult<DecryptedUserKey<T::PrivateKey, T::PublicKey>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_user_keys: Vec<DecryptedUserKey<T::PrivateKey, T::PublicKey>> =
            Vec::with_capacity(self.0.len());
        let mut decrypted_user_key_futures: Vec<_> = Vec::with_capacity(self.0.len());
        for locked_key in &self.0 {
            decrypted_user_key_futures.push(async {
                let decryption_result = provider
                    .private_key_import_async(
                        &locked_key.private_key,
                        salted_password,
                        DataEncoding::Armor,
                    )
                    .await;
                let private_key = decryption_result.map_err(|err| {
                    KeyError::Unlock(locked_key.id.clone(), AccountCryptoError::KeyImport(err))
                })?;
                let public_key = provider
                    .private_key_to_public_key_async(&private_key)
                    .await
                    .map_err(|err| {
                        KeyError::Unlock(
                            locked_key.id.clone(),
                            AccountCryptoError::TransformPublic(err),
                        )
                    })?;
                Ok(DecryptedUserKey {
                    private_key,
                    public_key,
                    id: locked_key.id.clone(),
                })
            });
        }
        let decrypted_user_key_results: Vec<_> = join_all(decrypted_user_key_futures).await;
        decrypted_user_keys.extend(decrypted_user_key_results.into_iter().filter_map(
            |decrypted_user_key_result| match decrypted_user_key_result {
                Ok(decrypted_user_key) => Some(decrypted_user_key),
                Err(err) => {
                    failed_keys.push(err);
                    None
                }
            },
        ));
        UnlockResult {
            unlocked_keys: decrypted_user_keys,
            failed: failed_keys,
        }
    }
}

/// Represents a decrypted user key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug, Clone)]
pub struct DecryptedUserKey<Priv: PrivateKey, Pub: PublicKey> {
    /// Proton key id.
    pub id: KeyId,
    /// PGP provider private key.
    pub private_key: Priv,
    /// PGP provider public key.
    pub public_key: Pub,
}

impl<Priv: PrivateKey, Pub: PublicKey> AsRef<Priv> for DecryptedUserKey<Priv, Pub> {
    fn as_ref(&self) -> &Priv {
        &self.private_key
    }
}

impl<Priv: PrivateKey, Pub: PublicKey> AsPublicKeyRef<Pub> for DecryptedUserKey<Priv, Pub> {
    fn as_public_key(&self) -> &Pub {
        &self.public_key
    }
}
