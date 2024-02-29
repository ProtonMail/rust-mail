use crate::keyring::{KeyId, LockedKey};
use crate::salts::SaltedPassword;
use proton_crypto::crypto::{DataEncoding, PrivateKey};
use serde::Deserialize;

pub struct PrivateKeyRing<T: PrivateKey>(Vec<T>);

impl<T: PrivateKey> AsRef<[T]> for PrivateKeyRing<T> {
    fn as_ref(&self) -> &[T] {
        self.0.as_ref()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Could not unlock key {0}:{1}")]
    Unlock(KeyId, Box<dyn std::error::Error>),
    #[error("Could not add key {0} to KeyRing: {1}")]
    AddKey(KeyId, Box<dyn std::error::Error>),
    #[error("No keys were unlocked")]
    NoKeysUnlocked,
    #[error("Missing encryption token or signature for key {0}")]
    MissingTokenOrSignature(KeyId),
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
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

    pub fn unlock<T: proton_crypto::crypto::PGPProviderSync>(
        &self,
        provider: &T,
        salted_password: &SaltedPassword<impl AsRef<[u8]>>,
    ) -> Result<PrivateKeyRing<T::PrivateKey>, KeyError> {
        let mut kr = PrivateKeyRing(Vec::with_capacity(self.0.len()));
        for locked_key in &self.0 {
            let key = provider
                .private_key_import(
                    &locked_key.private_key,
                    salted_password,
                    DataEncoding::Armor,
                )
                .map_err(|e| KeyError::Unlock(locked_key.id.clone(), e))?;
            kr.0.push(key)
        }
        Ok(kr)
    }

    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        salted_password: &SaltedPassword<impl AsRef<[u8]>>,
    ) -> Result<PrivateKeyRing<T::PrivateKey>, KeyError> {
        let mut kr = PrivateKeyRing(Vec::with_capacity(self.0.len()));
        for locked_key in &self.0 {
            let key = provider
                .private_key_import_async(
                    &locked_key.private_key,
                    salted_password,
                    DataEncoding::Armor,
                )
                .await
                .map_err(|e| KeyError::Unlock(locked_key.id.clone(), e))?;
            kr.0.push(key)
        }
        Ok(kr)
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
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

    pub fn unlock<T: proton_crypto::crypto::PGPProviderSync>(
        &self,
        provider: &T,
        user_key_ring: &PrivateKeyRing<T::PrivateKey>,
    ) -> Result<PrivateKeyRing<T::PrivateKey>, KeyError> {
        let mut kr = PrivateKeyRing(Vec::new());
        for locked_key in &self.0 {
            let (Some(token), Some(signature)) = (&locked_key.token, &locked_key.signature) else {
                return Err(KeyError::MissingTokenOrSignature(locked_key.id.clone()));
            };

            let key = provider
                .private_key_import_from_token(
                    &locked_key.private_key,
                    user_key_ring,
                    token,
                    signature,
                )
                .map_err(|e| KeyError::Unlock(locked_key.id.clone(), e))?;
            kr.0.push(key)
        }
        Ok(kr)
    }

    pub async fn unlock_async<T: proton_crypto::crypto::PGPProviderAsync>(
        &self,
        provider: &T,
        user_key_ring: &PrivateKeyRing<T::PrivateKey>,
    ) -> Result<PrivateKeyRing<T::PrivateKey>, KeyError> {
        let mut kr = PrivateKeyRing(Vec::new());
        for locked_key in &self.0 {
            let (Some(token), Some(signature)) = (&locked_key.token, &locked_key.signature) else {
                return Err(KeyError::MissingTokenOrSignature(locked_key.id.clone()));
            };

            let key = provider
                .private_key_import_from_token_async(
                    &locked_key.private_key,
                    user_key_ring,
                    token,
                    signature,
                )
                .await
                .map_err(|e| KeyError::Unlock(locked_key.id.clone(), e))?;
            kr.0.push(key)
        }
        Ok(kr)
    }
}
