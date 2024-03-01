use futures::future::join_all;

use crate::keys::{APIPublicKey, APIPublicKeySource, KeyFlag, KeyId, LockedKey};
use crate::salts::SaltedPassword;
use proton_crypto::crypto::{DataEncoding, PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};

/// Represents a decrypted user key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug)]
pub struct DecryptedUserKey<Priv: PrivateKey> {
    pub id: KeyId,
    pub private_keys: Priv,
}

impl<Priv: PrivateKey> AsRef<Priv> for DecryptedUserKey<Priv> {
    fn as_ref(&self) -> &Priv {
        &self.private_keys
    }
}

/// Represents a decrypted address key of a user.
///
/// Contains secret key material that must be protected.
#[derive(Debug)]
pub struct DecryptedAddressKey<Priv: PrivateKey> {
    pub id: KeyId,
    pub flags: KeyFlag,
    pub primary: bool,
    pub private_keys: Priv,
}

impl<Priv: PrivateKey> AsRef<Priv> for DecryptedAddressKey<Priv> {
    fn as_ref(&self) -> &Priv {
        &self.private_keys
    }
}

/// Represents a public address key of another user.
///
/// Public address keys are used to verify signatures or encrypt to addresses of other users.
/// Only contains public information and not secret key material.
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
    ) -> Vec<DecryptedUserKey<T::PrivateKey>> {
        let decrypted_user_keys = self
            .0
            .iter()
            .filter(|key| key.active)
            .filter_map(|key| {
                let decryption_result = provider.private_key_import(
                    &key.private_key,
                    salted_password,
                    DataEncoding::Armor,
                );
                let Ok(private_key) = decryption_result else {
                    return None;
                };
                Some(DecryptedUserKey {
                    private_keys: private_key,
                    id: key.id.clone(),
                })
            })
            .collect();
        decrypted_user_keys
    }

    /// Unlocks/decrypts the locked keys with the salted_password.
    ///
    /// Returns the user keys that have been successfully decrypted with the
    /// provided password. If decryption fails, a key is ignored.
    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        salted_password: &SaltedPassword<impl AsRef<[u8]>>,
    ) -> Vec<DecryptedUserKey<T::PrivateKey>> {
        let decrypted_user_keys_futures: Vec<_> = self
            .0
            .iter()
            .map(|key| {
                provider.private_key_import_async(
                    &key.private_key,
                    salted_password,
                    DataEncoding::Armor,
                )
            })
            .collect();
        let decrypted_keys_result: Vec<_> = join_all(decrypted_user_keys_futures).await;
        let decrypted_user_keys: Vec<_> = decrypted_keys_result
            .into_iter()
            .zip(&self.0)
            .filter_map(|(decryption_result, key)| {
                let Ok(private_key) = decryption_result else {
                    return None;
                };
                Some(DecryptedUserKey {
                    private_keys: private_key,
                    id: key.id.clone(),
                })
            })
            .collect();
        decrypted_user_keys
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
        user_keys: impl AsRef<[DecryptedUserKey<T::PrivateKey>]>,
    ) -> Vec<DecryptedAddressKey<T::PrivateKey>> {
        let decrypted_address_keys = self
            .0
            .iter()
            .filter(|key| key.active)
            .filter_map(|locked_key| {
                let (Some(token), Some(signature), Some(flags)) =
                    (&locked_key.token, &locked_key.signature, &locked_key.flags)
                else {
                    return None;
                };
                let decryption_result = provider.private_key_import_from_token(
                    &locked_key.private_key,
                    user_keys.as_ref(),
                    token,
                    signature,
                );
                let Ok(private_key) = decryption_result else {
                    return None;
                };
                Some(DecryptedAddressKey {
                    private_keys: private_key,
                    id: locked_key.id.clone(),
                    flags: KeyFlag::from(*flags),
                    primary: locked_key.primary,
                })
            })
            .collect();
        decrypted_address_keys
    }
    /// Decrypts the address keys with the provided user keys asynchronously.
    ///
    /// Returns the address keys that were successfully decrypted and verified using the provided user keys.
    /// If decryption or verification fails for a key, the key is not included in the returned vector.
    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        user_keys: impl AsRef<[DecryptedUserKey<T::PrivateKey>]>,
    ) -> Vec<DecryptedAddressKey<T::PrivateKey>> {
        let valid_keys: Vec<_> = self
            .0
            .iter()
            .filter(|locked_key| {
                locked_key.token.is_some()
                    && locked_key.signature.is_some()
                    && locked_key.flags.is_some()
            })
            .collect();
        let decrypted_address_keys_futures: Vec<_> = valid_keys
            .iter()
            .filter_map(|locked_key| {
                let (Some(token), Some(signature)) = (&locked_key.token, &locked_key.signature)
                else {
                    return None;
                };
                Some(provider.private_key_import_from_token_async(
                    &locked_key.private_key,
                    user_keys.as_ref(),
                    token,
                    signature,
                ))
            })
            .collect();
        let decrypted_keys_result: Vec<_> = join_all(decrypted_address_keys_futures).await;
        let decrypted_address_keys: Vec<_> = decrypted_keys_result
            .into_iter()
            .zip(&valid_keys)
            .filter_map(|(decryption_result, locked_key)| {
                let Ok(private_key) = decryption_result else {
                    return None;
                };
                Some(DecryptedAddressKey {
                    private_keys: private_key,
                    id: locked_key.id.clone(),
                    flags: KeyFlag::from(locked_key.flags.unwrap()),
                    primary: locked_key.primary,
                })
            })
            .collect();
        decrypted_address_keys
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
    ) -> Vec<PublicAddressKey<T::PublicKey>> {
        self.0
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
            .collect()
    }
    /// Imports the public keys by decoding the pgp public keys with the PGP provider.
    ///
    /// Returns the successfully imported public keys.
    /// If the import fails for a public key, the public key is not included in the returned vector.
    pub async fn import_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
    ) -> Vec<PublicAddressKey<T::PublicKey>> {
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
        imported_keys
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
            .collect()
    }
}
