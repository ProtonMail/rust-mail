use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use crate::{CoreContextResult, UserContext};
use proton_api_core::{
    auth::UserKeySecret,
    exports::crypto::{
        domain::{
            DecryptedAddressKey, KeyFlag, UnlockedAddressKey, UnlockedAddressKeys, UnlockedUserKey,
            UnlockedUserKeys,
        },
        proton_crypto::crypto::{DataEncoding, PGPProviderSync},
    },
};
use proton_api_core::{
    domain::AddressId,
    exports::{
        crypto::{
            domain::{DecryptedUserKey, KeyId},
            proton_crypto::CryptoError,
        },
        parking_lot::RwLock,
        thiserror,
    },
};
use secrecy::{ExposeSecret, SecretVec};
use proton_api_core::domain::{Address, User};
use stash::orm::Model;

#[allow(clippy::module_name_repetitions)]
type CachedUserKeys = Vec<CachedUserKey>;
#[allow(clippy::module_name_repetitions)]
type CachedAddressKeys = Vec<CachedAddressKey>;

/// The default lifetime of user keys in the cache.
const USER_KEY_LIFETIME: Duration = Duration::from_secs(600);

/// The default lifetime of address keys in the cache.
const ADDRESS_KEY_LIFETIME: Duration = Duration::from_secs(300);

/// A trait that loads the user secret to unlock the user keys.
pub trait LoadKeySecret {
    /// Loads the user secret to unlock the user keys.
    fn key_secret(&self) -> Option<UserKeySecret>;
}

/// Represents a cached user key independent of the PGP provider.
struct CachedUserKey {
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
struct CachedAddressKey {
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

/// An error type that is thrown when loading keys
/// via the [`CryptoKeyManager`].
#[derive(Debug, thiserror::Error)]
pub enum KeyHandlingError {
    #[error("No user found")]
    NoUser,
    #[error("No user secret found")]
    NoUserSecret,
    #[error("No user keys unlocked but has {0} user keys")]
    UserKeyUnlock(usize),
    #[error("Failed to store user keys in the cache {0}")]
    UserKeyCacheStore(#[from] CryptoError),
    #[error("No address found for id {0}")]
    NoAddress(AddressId),
    #[error("Failed to unlock at least one address key, but the user has {0} address keys")]
    AddressKeyUnlock(usize),
}

/// Manages an caches the PGP keys of a user.
pub struct CryptoKeyManager {
    /// Lifetime duration of user keys in the cache.
    user_key_lifetime: Duration,
    /// Lifetime duration of address keys in the cache.
    address_key_lifetime: Duration,
    /// A cache for user keys.
    user_keys: RwLock<CacheOption<CachedUserKeys>>,
    /// A cache for address keys.
    address_keys: RwLock<HashMap<AddressId, CacheOption<CachedAddressKeys>>>,
}

impl Default for CryptoKeyManager {
    fn default() -> Self {
        CryptoKeyManager {
            user_key_lifetime: USER_KEY_LIFETIME,
            address_key_lifetime: ADDRESS_KEY_LIFETIME,
            user_keys: RwLock::new(CacheOption::none()),
            address_keys: RwLock::new(HashMap::new()),
        }
    }
}

impl CryptoKeyManager {
    /// Creates a new default [`CryptoKeyManager`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the unlocked user keys of this user given its [`UserContext`].
    ///
    /// First tries to retrieve them from the cache else
    /// it loads and unlocks them from the database.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn user_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        secret_load: &impl LoadKeySecret,
        user_ctx: &UserContext,
    ) -> CoreContextResult<UnlockedUserKeys<Provider>> {
        let cached_keys = self.user_keys.read().get(self.user_key_lifetime);
        let unlocked_keys = match cached_keys {
            Some(cached_keys) => Self::load_user_keys_cache(pgp_provider, cached_keys.as_ref())?,
            None => self.load_user_keys_db(pgp_provider, secret_load, user_ctx).await?,
        };
        Ok(unlocked_keys)
    }

    /// Returns the unlocked address keys of this user for the given address and [`UserContext`].
    ///
    /// Loads the address keys from the database and unlocks them with the user keys.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn address_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        secret_load: &impl LoadKeySecret,
        user_ctx: &UserContext,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<Provider>> {
        let cached_keys = self
            .address_keys
            .read()
            .get(address_id)
            .and_then(|opt| opt.get(self.address_key_lifetime));
        let unlocked_keys = match cached_keys {
            Some(cached_keys) => Self::load_address_keys_cache(pgp_provider, cached_keys.as_ref())?,
            None => self.load_address_keys_db(pgp_provider, secret_load, user_ctx, address_id).await?,
        };
        Ok(unlocked_keys)
    }

    /// Clears the user key cache.
    pub fn clear_user_key_cache(&self) {
        let mut user_keys_ref_mut = self.user_keys.write();
        *user_keys_ref_mut = CacheOption::none();
    }

    /// Clears the address key cache.
    pub fn clear_address_key_cache(&self) {
        self.address_keys.write().clear();
    }

    /// Clears the address key cache for a specific address id.
    pub fn clear_item_address_key_cache(&self, address_id: &AddressId) {
        self.address_keys.write().remove(address_id);
    }

    /// Clears all internal caches.
    pub fn clear_cache(&self) {
        self.clear_user_key_cache();
        self.clear_address_key_cache();
    }

    /// Helper function to update the user keys in the internal cache.
    fn update_user_key_cache<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        keys: &[UnlockedUserKey<Provider>],
    ) -> Result<(), KeyHandlingError> {
        let mut new_cached_keys: CachedUserKeys = Vec::with_capacity(keys.len());
        for key in keys {
            let cached_key = CachedUserKey::new(pgp_provider, key)?;
            new_cached_keys.push(cached_key);
        }
        let mut mut_ref = self.user_keys.write();
        *mut_ref = CacheOption::new(new_cached_keys);
        Ok(())
    }

    /// Helper function to update the address keys in the internal cache.
    fn update_address_key_cache<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        address_id: &AddressId,
        keys: &[UnlockedAddressKey<Provider>],
    ) -> Result<(), KeyHandlingError> {
        let mut new_cached_keys: CachedAddressKeys = Vec::with_capacity(keys.len());
        for key in keys {
            let cached_key = CachedAddressKey::new(pgp_provider, key)?;
            new_cached_keys.push(cached_key);
        }
        let mut mut_ref = self.address_keys.write();
        mut_ref.insert(address_id.clone(), CacheOption::new(new_cached_keys));
        // Remove old keys from the cache.
        let obsolete_keys: Vec<_> = mut_ref
            .iter()
            .filter_map(|(address_id, opt)| {
                opt.get(self.address_key_lifetime)
                    .map_or(Some(address_id.clone()), |_| None)
            })
            .collect();
        for key in obsolete_keys {
            mut_ref.remove(&key);
        }
        Ok(())
    }

    /// Helper function to load user keys from the internal cache.
    fn load_user_keys_cache<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        user_keys: &CachedUserKeys,
    ) -> Result<UnlockedUserKeys<Provider>, KeyHandlingError> {
        let mut unlocked_user_keys = Vec::with_capacity(user_keys.len());
        for key in user_keys {
            let imported_key = key.to_unlocked_key(pgp_provider)?;
            unlocked_user_keys.push(imported_key);
        }
        Ok(unlocked_user_keys)
    }

    /// Helper function to load address keys from the internal cache.
    fn load_address_keys_cache<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        address_keys: &CachedAddressKeys,
    ) -> Result<UnlockedAddressKeys<Provider>, KeyHandlingError> {
        let mut unlocked_address_keys = Vec::with_capacity(address_keys.len());
        for key in address_keys {
            let imported_key = key.to_unlocked_key(pgp_provider)?;
            unlocked_address_keys.push(imported_key);
        }
        Ok(unlocked_address_keys)
    }

    /// Helper function to load and unlock user address keys from the DB.
    ///
    /// This function acquires a write lock on `self.address_keys` to update the cache.
    async fn load_address_keys_db<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        secret_load_fn: &impl LoadKeySecret,
        user_ctx: &UserContext,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<Provider>> {
        // Load the address from the DB.
        let address = Address::load(address_id.clone(), &user_ctx.stash).await?
            .ok_or(KeyHandlingError::NoAddress(address_id.clone()))?;
        // Load the user keys.
        let user_keys = self.user_keys(pgp_provider, secret_load_fn, user_ctx).await?;
        // Unlock the address keys.
        let unlock_result = address.keys.unlock(pgp_provider, &user_keys);
        if unlock_result.unlocked_keys.is_empty() {
            return Err(KeyHandlingError::AddressKeyUnlock(unlock_result.failed.len()).into());
        }
        // Update the cache.
        self.update_address_key_cache(pgp_provider, address_id, &unlock_result.unlocked_keys)?;
        Ok(unlock_result.unlocked_keys)
    }

    /// Helper function to load and unlock user keys from the DB.
    ///
    /// This function acquires a write lock on `self.user_keys` to update the cache.
    async fn load_user_keys_db<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        secret_loader: &impl LoadKeySecret,
        user_ctx: &UserContext,
    ) -> CoreContextResult<UnlockedUserKeys<Provider>> {
        // Load the user from the DB.
        let user = User::load(user_ctx.user_id().clone(), &user_ctx.stash).await?
            .ok_or(KeyHandlingError::NoUser)?;
        // Load the user secret to unlock the key.
        let pw = secret_loader
            .key_secret()
            .ok_or(KeyHandlingError::NoUserSecret)?;
        // Unlock the keys.
        let unlock_result = user.unlock_keys(pgp_provider, pw.expose_secret());
        if unlock_result.unlocked_keys.is_empty() {
            return Err(KeyHandlingError::UserKeyUnlock(unlock_result.failed.len()).into());
        }
        // Update the cache.
        self.update_user_key_cache(pgp_provider, &unlock_result.unlocked_keys)?;
        Ok(unlock_result.unlocked_keys)
    }
}

impl UserContext {
    /// Returns the unlocked user keys of this user.
    ///
    /// First tries to retrieve them from the cache else
    /// it loads and unlocks them from the database.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn user_keys_unlocked<Provider: PGPProviderSync, Secret: LoadKeySecret>(
        &self,
        pgp_provider: &Provider,
        secret_loader: &Secret,
    ) -> CoreContextResult<UnlockedUserKeys<Provider>> {
        self.key_manager
            .user_keys(pgp_provider, secret_loader, self).await
    }

    /// Returns the unlocked address keys of this user for the given address.
    ///
    /// Loads the address keys from the database and unlocks them with the user keys.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn address_keys_unlocked<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        secret_load_fn: &impl LoadKeySecret,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<Provider>> {
        self.key_manager
            .address_keys(pgp_provider, secret_load_fn, self, address_id).await
    }
}

pub struct CacheOption<T>(Option<(Instant, Arc<T>)>);

impl<T> CacheOption<T> {
    fn new(item: T) -> Self {
        Self(Some((Instant::now(), Arc::new(item))))
    }

    fn none() -> Self {
        Self(None)
    }

    fn get(&self, lifetime: Duration) -> Option<Arc<T>> {
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
