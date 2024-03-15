use futures::future::join_all;

use super::{APIPublicKey, APIPublicKeySource, KeyFlag};
use proton_crypto::crypto::{AsPublicKeyRef, DataEncoding, PublicKey};
use serde::Deserialize;

/// Represents public address keys retrieved from the API.
#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
pub struct APIPublicAddressKeys(pub Vec<APIPublicKey>);

impl AsRef<[APIPublicKey]> for APIPublicAddressKeys {
    fn as_ref(&self) -> &[APIPublicKey] {
        &self.0
    }
}

/// Represents a public address key of another user.
///
/// Public address keys are used to verify signatures or encrypt to addresses of other users.
/// Only contains public information and no secret key material.
#[derive(Debug, Clone)]
pub struct PublicAddressKey<Pub: PublicKey> {
    /// Origin of the public key.
    pub source: APIPublicKeySource,
    /// Key flags encoded in a bitmap.
    pub flags: KeyFlag,
    /// The imported PGP provider public key.
    pub public_keys: Pub,
}

impl<Pub: PublicKey> AsPublicKeyRef<Pub> for PublicAddressKey<Pub> {
    fn as_public_key(&self) -> &Pub {
        &self.public_keys
    }
}

/// Represents imported address public keys retrieved from the API.
#[derive(Debug)]
pub struct PublicAddressKeys<T: PublicKey>(pub Vec<PublicAddressKey<T>>);

impl<T: PublicKey> PublicAddressKeys<T> {
    pub fn as_slice(&self) -> &[PublicAddressKey<T>] {
        self.0.as_slice()
    }
}

impl<T: PublicKey> AsRef<[PublicAddressKey<T>]> for PublicAddressKeys<T> {
    fn as_ref(&self) -> &[PublicAddressKey<T>] {
        self.as_slice()
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
