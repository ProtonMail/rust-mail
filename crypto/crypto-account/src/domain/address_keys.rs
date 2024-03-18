use futures::future::join_all;

use super::{DecryptedUserKey, KeyError, KeyFlag, KeyId, LockedKey, UnlockResult};
use proton_crypto::crypto::{AsPublicKeyRef, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

/// Represents locked address keys of a user retrieved from the API.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct AddressKeys(pub Vec<LockedKey>);

impl AsRef<[LockedKey]> for AddressKeys {
    fn as_ref(&self) -> &[LockedKey] {
        &self.0
    }
}

impl AddressKeys {
    /// Creates new `AddressKeys` from an iterator of locked keys from the API.
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
        decrypted_address_keys.extend(self.0.iter().filter_map(|locked_key| {
            let (Some(token), Some(signature), Some(flags)) =
                (&locked_key.token, &locked_key.signature, &locked_key.flags)
            else {
                failed_keys.push(KeyError::MissingValue(locked_key.id.clone()));
                return None;
            };
            let decryption_result = crate::crypto::import_key_with_token(
                provider,
                &locked_key.private_key,
                token,
                signature,
                user_keys.as_ref(),
                user_keys.as_ref(),
                None,
            );
            let (private_key, public_key) = match decryption_result {
                Ok(key) => key,
                Err(err) => {
                    failed_keys.push(KeyError::UnlockToken(locked_key.id.clone(), err));
                    return None;
                }
            };
            Some(DecryptedAddressKey {
                private_key,
                public_key,
                id: locked_key.id.clone(),
                flags: *flags,
                primary: locked_key.primary.into(),
            })
        }));
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
                let decryption_result = crate::crypto::import_key_with_token_async(
                    provider,
                    &locked_key.private_key,
                    token,
                    signature,
                    user_keys.as_ref(),
                    user_keys.as_ref(),
                    None,
                )
                .await;
                let (private_key, public_key) = decryption_result
                    .map_err(|err| KeyError::UnlockToken(locked_key.id.clone(), err))?;
                Ok(DecryptedAddressKey {
                    private_key,
                    public_key,
                    id: locked_key.id.clone(),
                    flags: *flags,
                    primary: locked_key.primary.into(),
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

/// Represents a decrypted address key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug, Clone)]
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
    fn as_public_key(&self) -> &Pub {
        &self.public_key
    }
}
