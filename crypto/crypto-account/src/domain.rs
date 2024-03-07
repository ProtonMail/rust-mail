use futures::future::join_all;

use crate::keys::{APIPublicKey, APIPublicKeySource, KeyFlag, KeyId, LockedKey};
use crate::salts::SaltedPassword;
use proton_crypto::crypto::{AsPublicKeyRef, DataEncoding, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Could not unlock key {0}:{1}")]
    Unlock(KeyId, Box<dyn std::error::Error>),
    #[error("Missing encryption token, signature, or flags for key {0}")]
    MissingValue(KeyId),
    #[error("Failed to extract public key from {0}:{1}")]
    PublicKeyExtraction(KeyId, Box<dyn std::error::Error>),
}

/// Represents a decrypted user key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug)]
pub struct DecryptedUserKey<Priv: PrivateKey, Pub: PublicKey> {
    pub id: KeyId,
    pub private_key: Priv,
    pub public_key: Pub,
}

impl<Priv: PrivateKey, Pub: PublicKey> AsRef<Priv> for DecryptedUserKey<Priv, Pub> {
    fn as_ref(&self) -> &Priv {
        &self.private_key
    }
}

impl<Priv: PrivateKey, Pub: PublicKey> AsPublicKeyRef<Pub> for DecryptedUserKey<Priv, Pub> {
    fn as_public_key_ref(&self) -> &Pub {
        &self.public_key
    }
}

/// Represents a decrypted address key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug)]
pub struct DecryptedAddressKey<Priv: PrivateKey, Pub: PublicKey> {
    pub id: KeyId,
    pub flags: KeyFlag,
    pub primary: bool,
    pub private_key: Priv,
    pub public_key: Pub,
}

impl<Priv: PrivateKey, Pub: PublicKey> AsRef<Priv> for DecryptedAddressKey<Priv, Pub> {
    fn as_ref(&self) -> &Priv {
        &self.private_key
    }
}

impl<Priv: PrivateKey, Pub: PublicKey> AsPublicKeyRef<Pub> for DecryptedAddressKey<Priv, Pub> {
    fn as_public_key_ref(&self) -> &Pub {
        &self.public_key
    }
}

/// Represents a public address key of another user.
///
/// Public address keys are used to verify signatures or encrypt to addresses of other users.
/// Only contains public information and no secret key material.
#[derive(Debug)]
pub struct PublicAddressKey<Pub: PublicKey> {
    pub source: APIPublicKeySource,
    pub flags: KeyFlag,
    pub public_keys: Pub,
}

impl<Pub: PublicKey> AsRef<Pub> for PublicAddressKey<Pub> {
    fn as_ref(&self) -> &Pub {
        &self.public_keys
    }
}

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

    /// Unlocks/decrypts the locked keys with the provided salted_password.
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
        decrypted_address_keys.extend(self.0.iter().filter(|key| key.active).filter_map(
            |locked_key| {
                let decryption_result = provider.private_key_import(
                    &locked_key.private_key,
                    salted_password,
                    DataEncoding::Armor,
                );
                let private_key = match decryption_result {
                    Ok(key) => key,
                    Err(err) => {
                        failed_keys.push(KeyError::Unlock(locked_key.id.clone(), err));
                        return None;
                    }
                };
                let public_key = match provider.private_key_to_public_key(&private_key) {
                    Ok(key) => key,
                    Err(err) => {
                        failed_keys.push(KeyError::PublicKeyExtraction(locked_key.id.clone(), err));
                        return None;
                    }
                };
                Some(DecryptedUserKey {
                    private_key,
                    public_key,
                    id: locked_key.id.clone(),
                })
            },
        ));
        UnlockResult {
            unlocked_keys: decrypted_address_keys,
            failed: failed_keys,
        }
    }

    /// Unlocks/decrypts the locked keys with the salted_password.
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
                let private_key = decryption_result
                    .map_err(|err| KeyError::Unlock(locked_key.id.clone(), err))?;
                let public_key = provider
                    .private_key_to_public_key_async(&private_key)
                    .await
                    .map_err(|err| KeyError::PublicKeyExtraction(locked_key.id.clone(), err))?;
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

pub struct UnlockResult<T> {
    pub unlocked_keys: Vec<T>,
    pub failed: Vec<KeyError>,
}

impl<T> From<UnlockResult<T>> for Vec<T> {
    fn from(value: UnlockResult<T>) -> Self {
        value.unlocked_keys
    }
}

/// Represents locked address keys of a user retrieved from the API.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct AddressKeys(pub Vec<LockedKey>);

impl AsRef<[LockedKey]> for AddressKeys {
    fn as_ref(&self) -> &[LockedKey] {
        &self.0
    }
}

impl AddressKeys {
    pub fn new(v: impl IntoIterator<Item = LockedKey>) -> Self {
        Self(Vec::from_iter(v))
    }
    /// Decrypts the address keys with the provided user keys.
    ///
    /// Returns the address keys that were successfully decrypted and verified using the provided user keys.
    /// If decryption or verification fails for a key, the key is not included in the returned vector.
    pub fn unlock<T: proton_crypto::crypto::PGPProviderSync>(
        &self,
        provider: &T,
        user_keys: impl AsRef<[DecryptedUserKey<T::PrivateKey, T::PublicKey>]>,
    ) -> UnlockResult<DecryptedAddressKey<T::PrivateKey, T::PublicKey>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_address_keys: Vec<DecryptedAddressKey<_, _>> =
            Vec::with_capacity(self.0.len());
        decrypted_address_keys.extend(self.0.iter().filter(|key| key.active).filter_map(
            |locked_key| {
                let (Some(token), Some(signature), Some(flags)) =
                    (&locked_key.token, &locked_key.signature, &locked_key.flags)
                else {
                    failed_keys.push(KeyError::MissingValue(locked_key.id.clone()));
                    return None;
                };
                let decryption_result = provider.private_key_import_from_token_refs(
                    &locked_key.private_key,
                    user_keys.as_ref(),
                    token,
                    signature,
                );
                let private_key = match decryption_result {
                    Ok(key) => key,
                    Err(err) => {
                        failed_keys.push(KeyError::Unlock(locked_key.id.clone(), err));
                        return None;
                    }
                };
                let public_key = match provider.private_key_to_public_key(&private_key) {
                    Ok(key) => key,
                    Err(err) => {
                        failed_keys.push(KeyError::PublicKeyExtraction(locked_key.id.clone(), err));
                        return None;
                    }
                };
                Some(DecryptedAddressKey {
                    private_key,
                    public_key,
                    id: locked_key.id.clone(),
                    flags: KeyFlag::from(*flags),
                    primary: locked_key.primary,
                })
            },
        ));
        UnlockResult {
            unlocked_keys: decrypted_address_keys,
            failed: failed_keys,
        }
    }
    /// Decrypts the address keys with the provided user keys asynchronously.
    ///
    /// Returns the address keys that were successfully decrypted and verified using the provided user keys.
    /// If decryption or verification fails for a key, the key is not included in the returned vector.
    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        user_keys: impl AsRef<[DecryptedUserKey<T::PrivateKey, T::PublicKey>]>,
    ) -> UnlockResult<DecryptedAddressKey<T::PrivateKey, T::PublicKey>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_address_keys: Vec<DecryptedAddressKey<_, _>> =
            Vec::with_capacity(self.0.len());
        let mut decrypted_address_key_futures: Vec<_> = Vec::with_capacity(self.0.len());
        for locked_key in &self.0 {
            decrypted_address_key_futures.push(async {
                let (Some(token), Some(signature), Some(flags)) =
                    (&locked_key.token, &locked_key.signature, &locked_key.flags)
                else {
                    return Err(KeyError::MissingValue(locked_key.id.clone()));
                };
                let decryption_result = provider
                    .private_key_import_from_token_refs_async(
                        &locked_key.private_key,
                        user_keys.as_ref(),
                        token,
                        signature,
                    )
                    .await;
                let private_key = decryption_result
                    .map_err(|err| KeyError::Unlock(locked_key.id.clone(), err))?;
                let public_key = provider
                    .private_key_to_public_key_async(&private_key)
                    .await
                    .map_err(|err| KeyError::PublicKeyExtraction(locked_key.id.clone(), err))?;

                Ok(DecryptedAddressKey {
                    private_key,
                    public_key,
                    id: locked_key.id.clone(),
                    flags: KeyFlag::from(*flags),
                    primary: locked_key.primary,
                })
            });
        }
        let decrypted_address_key_results: Vec<_> = join_all(decrypted_address_key_futures).await;
        decrypted_address_keys.extend(decrypted_address_key_results.into_iter().filter_map(
            |decrypted_user_key_result| match decrypted_user_key_result {
                Ok(decrypted_user_key) => Some(decrypted_user_key),
                Err(err) => {
                    failed_keys.push(err);
                    None
                }
            },
        ));
        UnlockResult {
            unlocked_keys: decrypted_address_keys,
            failed: failed_keys,
        }
    }
}

/// Represents public address keys retrieved from the API.
#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
pub struct APIPublicAddressKeys(pub Vec<APIPublicKey>);

impl AsRef<[APIPublicKey]> for APIPublicAddressKeys {
    fn as_ref(&self) -> &[APIPublicKey] {
        &self.0
    }
}

impl APIPublicAddressKeys {
    /// Imports the public keys by decoding the pgp public keys with the PGP provider.
    ///
    /// Returns the successfully imported public keys.
    /// If the import fails for a public key, the public key is not included in the returned vector.
    pub fn import<T: proton_crypto::crypto::PGPProviderSync>(
        &self,
        provider: &T,
    ) -> PublicAddressKeys<T::PublicKey> {
        let public_address_keys = self
            .0
            .iter()
            .filter_map(|api_public_key| {
                let imported_public_key = provider
                    .public_key_import(api_public_key.public_key.as_bytes(), DataEncoding::Armor);
                let Ok(public_key) = imported_public_key else {
                    return None;
                };
                Some(PublicAddressKey {
                    source: api_public_key.source,
                    flags: api_public_key.flags,
                    public_keys: public_key,
                })
            })
            .collect();
        PublicAddressKeys(public_address_keys)
    }
    /// Imports the public keys by decoding the pgp public keys with the PGP provider.
    ///
    /// Returns the successfully imported public keys.
    /// If the import fails for a public key, the public key is not included in the returned vector.
    pub async fn import_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
    ) -> PublicAddressKeys<T::PublicKey> {
        let imported_keys_futures: Vec<_> = self
            .0
            .iter()
            .map(|api_public_key| {
                provider.public_key_import_async(
                    api_public_key.public_key.as_bytes(),
                    DataEncoding::Armor,
                )
            })
            .collect();
        let imported_keys: Vec<_> = join_all(imported_keys_futures).await;
        let public_address_keys = imported_keys
            .into_iter()
            .zip(&self.0)
            .filter_map(|(imported_key_result, api_public_key)| {
                let Ok(imported_key) = imported_key_result else {
                    return None;
                };
                Some(PublicAddressKey {
                    source: api_public_key.source,
                    flags: api_public_key.flags,
                    public_keys: imported_key,
                })
            })
            .collect();
        PublicAddressKeys(public_address_keys)
    }
}

/// Represents imported address public keys retrieved from the API.
#[derive(Debug)]
pub struct PublicAddressKeys<T: PublicKey>(pub Vec<PublicAddressKey<T>>);

impl<T: PublicKey> AsRef<[PublicAddressKey<T>]> for PublicAddressKeys<T> {
    fn as_ref(&self) -> &[PublicAddressKey<T>] {
        self.0.as_slice()
    }
}
