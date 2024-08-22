use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use proton_crypto_account::{
    keys::{
        DecryptedAddressKey, DecryptedUserKey, KeyFlag, KeyId, UnlockedAddressKey, UnlockedUserKey,
    },
    proton_crypto::crypto::{DataEncoding, PGPProviderSync},
};
use secrecy::SecretVec;

use secrecy::ExposeSecret;

use crate::KeyHandlingError;

/// The default lifetime of user keys in the cache.
pub const USER_KEY_LIFETIME: Duration = Duration::from_secs(600);

/// The default lifetime of address keys in the cache.
pub const ADDRESS_KEY_LIFETIME: Duration = Duration::from_secs(300);

/// Represents a cached user key independent of the PGP provider.
pub struct CachedUserKey {
    id: KeyId,
    private_key: SecretVec<u8>,
}

impl CachedUserKey {
    /// Tries to create a [`CachedUserKey`] from an [`UnlockedUserKey`].
    pub fn new<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        key: &UnlockedUserKey<Provider>,
    ) -> Result<CachedUserKey, KeyHandlingError> {
        let exported_key =
            pgp_provider.private_key_export_unlocked(&key.private_key, DataEncoding::Bytes)?;
        Ok(CachedUserKey {
            id: key.id.clone(),
            private_key: SecretVec::new(exported_key.as_ref().to_vec()),
        })
    }

    /// Tries to transform a [`CachedUserKey`] into a [`UnlockedUserKey`].
    pub fn to_unlocked_key<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> Result<UnlockedUserKey<Provider>, KeyHandlingError> {
        let imported_key = pgp_provider
            .private_key_import_unlocked(self.private_key.expose_secret(), DataEncoding::Bytes)?;
        let public_key = pgp_provider.private_key_to_public_key(&imported_key)?;
        Ok(DecryptedUserKey {
            id: self.id.clone(),
            private_key: imported_key,
            public_key,
        })
    }
}

/// Represents a cached address key independent of the PGP provider.
pub struct CachedAddressKey {
    id: KeyId,
    flags: KeyFlag,
    primary: bool,
    private_key: SecretVec<u8>,
}

impl CachedAddressKey {
    /// Tries to create a [`CachedAddressKey`] from an [`UnlockedAddressKey`].
    pub fn new<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        key: &UnlockedAddressKey<Provider>,
    ) -> Result<CachedAddressKey, KeyHandlingError> {
        let exported_key =
            pgp_provider.private_key_export_unlocked(&key.private_key, DataEncoding::Bytes)?;
        Ok(CachedAddressKey {
            id: key.id.clone(),
            flags: key.flags,
            primary: key.primary,
            private_key: SecretVec::new(exported_key.as_ref().to_vec()),
        })
    }

    /// Tries to transform a [`CachedAddressKey`] into a [`UnlockedAddressKey`].
    pub fn to_unlocked_key<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> Result<UnlockedAddressKey<Provider>, KeyHandlingError> {
        let imported_key = pgp_provider
            .private_key_import_unlocked(self.private_key.expose_secret(), DataEncoding::Bytes)?;
        let public_key = pgp_provider.private_key_to_public_key(&imported_key)?;
        Ok(DecryptedAddressKey {
            id: self.id.clone(),
            flags: self.flags,
            primary: self.primary,
            public_key,
            private_key: imported_key,
        })
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct CacheOption<T>(Option<(Instant, Arc<T>)>);

impl<T> CacheOption<T> {
    pub fn new(item: T) -> Self {
        Self(Some((Instant::now(), Arc::new(item))))
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn get(&self, lifetime: Duration) -> Option<Arc<T>> {
        match &self.0 {
            Some((cache, value)) => {
                let item_lifetime = cache.elapsed();
                if item_lifetime > lifetime {
                    return None;
                }
                Some(Arc::clone(value))
            }
            None => None,
        }
    }
}
