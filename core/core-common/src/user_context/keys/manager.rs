use super::KeyHandlingError;
use super::KeyHandlingResult;
use super::LoadKeySecret;
use super::{
    CachedAddressKeys, CachedUserKeys,
    cache::{
        ADDRESS_KEY_LIFETIME, CacheOption, CachedAddressKey, CachedUserKey, USER_KEY_LIFETIME,
    },
};
use crate::datatypes::UnixTimestamp;
use crate::models::Address;
use crate::models::{ModelIdExtension, User};
use crate::{CoreContextError, CoreContextResult, UserContext};
use indoc::indoc;
use parking_lot::RwLock;
use proton_core_api::consts::CoreBundle;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{AddressId, UserId};
use proton_core_api::services::proton::{GetKeysAllOptions, PrivateEmailRef};
use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicAddressKeys, PublicAddressKeys,
};
use proton_crypto_account::keys::{
    UnlockedAddressKey, UnlockedAddressKeys, UnlockedUserKey, UnlockedUserKeys,
};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use serde::{Deserialize, Serialize};
use stash::macros::DbRecord;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};
use stash::{params, sql_using_serde};
use std::{collections::HashMap, time::Duration};

/// Manages an caches the PGP keys.
#[allow(clippy::module_name_repetitions)]
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

#[derive(Debug, Copy, Clone, Default)]
pub enum PublicAddressKeyFetchPolicy {
    #[default]
    RequireSync,
    AllowCachedFallback,
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
    pub async fn user_keys<P>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_load: &impl LoadKeySecret,
        user_id: &UserId,
    ) -> CoreContextResult<UnlockedUserKeys<P>>
    where
        P: PGPProviderSync,
    {
        let cached_keys = self.user_keys.read().get(self.user_key_lifetime);

        let unlocked_keys = match cached_keys {
            Some(cached_keys) => Self::load_user_keys_cache(pgp, cached_keys.as_ref())?,
            None => {
                self.load_user_keys_db(pgp, conn, secret_load, user_id)
                    .await?
            }
        };

        Ok(unlocked_keys)
    }

    /// Returns the unlocked address keys of this user for the given address and [`UserContext`].
    ///
    /// Loads the address keys from the database and unlocks them with the user keys.
    pub async fn address_keys<P>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_load: &impl LoadKeySecret,
        user_id: &UserId,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<P>>
    where
        P: PGPProviderSync,
    {
        let cached_keys = self
            .address_keys
            .read()
            .get(address_id)
            .and_then(|opt| opt.get(self.address_key_lifetime));

        let unlocked_keys = match cached_keys {
            Some(cached_keys) => Self::load_address_keys_cache(pgp, cached_keys.as_ref())?,
            None => {
                self.load_address_keys_db(pgp, conn, secret_load, user_id, address_id)
                    .await?
            }
        };

        Ok(unlocked_keys)
    }

    /// Loads the public address keys for an email address from the backend API.
    ///
    /// Imports the keys with the PGP provider. In the future, this function will also
    /// verify the keys with key transparency.
    pub async fn public_address_keys<P>(
        &self,
        pgp: &P,
        email: PrivateEmailRef<'_>,
        internal_only: bool,
        fetch_policy: PublicAddressKeyFetchPolicy,
        user_context: &UserContext,
    ) -> CoreContextResult<PublicAddressKeys<<P>::PublicKey>>
    where
        P: PGPProviderSync,
    {
        let tether = user_context.user_stash.connection().await?;

        let cached_value = PublicAddressKeysResponseCache::get(
            email.as_clear_text_str().into(),
            internal_only,
            &tether,
        )
        .await?;

        let api_keys = match (cached_value, fetch_policy) {
            (Some(cached_response), PublicAddressKeyFetchPolicy::AllowCachedFallback)
                if cached_response.is_valid() =>
            {
                cached_response.into_response()
            }
            (v, policy) =>
            // Invalid or does not exist, we need to fetch
            {
                match user_context
                    .session()
                    .get_keys_all(GetKeysAllOptions {
                        email: email.to_owned(),
                        internal_only: Some(internal_only),
                    })
                    .await
                {
                    Ok(api_keys) => {
                        Self::store_public_key_request(
                            user_context,
                            &email,
                            internal_only,
                            &api_keys,
                        )
                        .await;
                        api_keys
                    }
                    Err(e)
                        if matches!(policy, PublicAddressKeyFetchPolicy::AllowCachedFallback)
                            && v.is_some()
                            && e.is_network_failure() =>
                    {
                        tracing::debug!("Using cached value due to network failure");
                        v.expect("validated as some").into_response()
                    }
                    // We need treat these specific errors when doing internal only queries as
                    // if they have no keys.
                    Err(ApiServiceError::UnprocessableEntity(_, Some(error)))
                        if internal_only
                            && error.code == CoreBundle::KeyGetAddressMissing as u32
                            || error.code == CoreBundle::KeyGetDomainExternal as u32 =>
                    {
                        let response = APIPublicAddressKeys {
                            address_keys: APIPublicAddressKeyGroup::default(),
                            catch_all_keys: None,
                            unverified_keys: None,
                            warnings: vec![],
                            proton_mx: false,
                            is_proton: false,
                        };
                        Self::store_public_key_request(
                            user_context,
                            &email,
                            internal_only,
                            &response,
                        )
                        .await;
                        response
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        };

        api_keys.import(pgp).map_err(|e| {
            tracing::error!("Failed to import public address keys: {e}");
            CoreContextError::Crypto
        })
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

    // Helper for storing a PublicAddressKeys API response in the cache
    async fn store_public_key_request(
        user_context: &UserContext,
        email: &PrivateEmailRef<'_>,
        internal_only: bool,
        response: &APIPublicAddressKeys,
    ) {
        if let Ok(mut tether) = user_context.user_stash.connection().await {
            if let Err(e) = tether
                .tx(async |tx| {
                    PublicAddressKeysResponseCache::store(
                        email.as_clear_text_str().to_owned(),
                        internal_only,
                        response.clone(),
                        tx,
                    )
                    .await
                })
                .await
            {
                tracing::error!("Failed to store response in cache: {e}");
            }
        } else {
            tracing::error!("Failed to get connection to store API key response in cache");
        }
    }

    /// Helper function to update the user keys in the internal cache.
    fn update_user_key_cache<P>(
        &self,
        pgp: &P,
        keys: &[UnlockedUserKey<P>],
    ) -> KeyHandlingResult<()>
    where
        P: PGPProviderSync,
    {
        let mut new_cached_keys: CachedUserKeys = Vec::with_capacity(keys.len());

        for key in keys {
            let cached_key = CachedUserKey::new(pgp, key)?;
            new_cached_keys.push(cached_key);
        }

        let mut mut_ref = self.user_keys.write();
        *mut_ref = CacheOption::new(new_cached_keys);

        Ok(())
    }

    /// Helper function to update the address keys in the internal cache.
    fn update_address_key_cache<P>(
        &self,
        pgp: &P,
        address_id: &AddressId,
        keys: &[UnlockedAddressKey<P>],
    ) -> KeyHandlingResult<()>
    where
        P: PGPProviderSync,
    {
        let mut new_cached_keys: CachedAddressKeys = Vec::with_capacity(keys.len());

        for key in keys {
            let cached_key = CachedAddressKey::new(pgp, key)?;
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
    fn load_user_keys_cache<P>(
        pgp: &P,
        user_keys: &CachedUserKeys,
    ) -> KeyHandlingResult<UnlockedUserKeys<P>>
    where
        P: PGPProviderSync,
    {
        let mut unlocked_user_keys = Vec::with_capacity(user_keys.len());

        for key in user_keys {
            let imported_key = key.to_unlocked_key(pgp)?;
            unlocked_user_keys.push(imported_key);
        }

        Ok(unlocked_user_keys.into())
    }

    /// Helper function to load address keys from the internal cache.
    fn load_address_keys_cache<P>(
        pgp: &P,
        address_keys: &CachedAddressKeys,
    ) -> KeyHandlingResult<UnlockedAddressKeys<P>>
    where
        P: PGPProviderSync,
    {
        let mut unlocked_address_keys = Vec::with_capacity(address_keys.len());

        for key in address_keys {
            let imported_key = key.to_unlocked_key(pgp)?;
            unlocked_address_keys.push(imported_key);
        }

        Ok(unlocked_address_keys.into())
    }

    /// Helper function to load and unlock user address keys from the DB.
    ///
    /// This function acquires a write lock on `self.address_keys` to update the cache.
    async fn load_address_keys_db<P>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_load_fn: &impl LoadKeySecret,
        user_id: &UserId,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<P>>
    where
        P: PGPProviderSync,
    {
        // Load the address from the DB.
        let address = Address::find_by_remote_id(address_id.clone(), conn)
            .await?
            .ok_or(KeyHandlingError::NoAddress(address_id.clone()))?;

        // Load the user keys.
        let user_keys = self.user_keys(pgp, conn, secret_load_fn, user_id).await?;

        let passphrase = secret_load_fn
            .key_secret()
            .await
            .map(|user_key_secret| user_key_secret.0);

        // Unlock the address keys.
        let unlock_result = address.keys.unlock(pgp, &user_keys, passphrase.as_ref());

        if unlock_result.unlocked_keys.is_empty() {
            return Err(CoreContextError::PGPKeyAccess(
                KeyHandlingError::AddressKeyUnlock(unlock_result.failed.len()),
            ));
        }

        // Update the cache.
        self.update_address_key_cache(pgp, address_id, &unlock_result.unlocked_keys)?;

        Ok(unlock_result.unlocked_keys.into())
    }

    /// Helper function to load and unlock user keys from the DB.
    ///
    /// This function acquires a write  lock on `self.user_keys` to update the cache.
    async fn load_user_keys_db<P>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_loader: &impl LoadKeySecret,
        user_id: &UserId,
    ) -> CoreContextResult<UnlockedUserKeys<P>>
    where
        P: PGPProviderSync,
    {
        // Load the user from the DB.
        let user = User::load(user_id.clone(), conn)
            .await?
            .ok_or(KeyHandlingError::NoUser)?;

        // Load the user secret to unlock the key.
        let pw = secret_loader
            .key_secret()
            .await
            .ok_or(KeyHandlingError::NoUserSecret)?;

        // Unlock the keys.
        let unlock_result = user.keys.unlock(pgp, pw.expose_secret());

        if unlock_result.unlocked_keys.is_empty() {
            return Err(CoreContextError::PGPKeyAccess(
                KeyHandlingError::UserKeyUnlock(unlock_result.failed.len()),
            ));
        }

        // Update the cache.
        self.update_user_key_cache(pgp, &unlock_result.unlocked_keys)?;

        Ok(unlock_result.unlocked_keys.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct PublicKeyResponseCacheValue(APIPublicAddressKeys);

sql_using_serde!(PublicKeyResponseCacheValue);

pub struct PublicAddressKeysResponseCache;

#[derive(Debug, DbRecord, PartialEq, Clone)]
pub struct PublickeyResponseCacheEntry {
    #[DbField]
    response: PublicKeyResponseCacheValue,
    #[DbField]
    timestamp: UnixTimestamp,
}

const PUBLIC_RESPONSE_KEY_RESPONSE_TTL_SEC: u64 = 60 * 60; //1h
impl PublickeyResponseCacheEntry {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.timestamp
            .saturating_add(PUBLIC_RESPONSE_KEY_RESPONSE_TTL_SEC)
            >= UnixTimestamp::now()
    }

    #[must_use]
    pub fn into_response(self) -> APIPublicAddressKeys {
        self.response.0
    }
}

impl PublicAddressKeysResponseCache {
    pub async fn store(
        email: String,
        internal_only: bool,
        response: APIPublicAddressKeys,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            indoc! {"
            INSERT INTO public_address_key_response_cache (email, internal_only, response, timestamp)
            VALUES (?,?,?, ?)
            ON CONFLICT (email, internal_only) DO UPDATE SET
                response=excluded.response,
                timestamp=excluded.timestamp
        "},
            params![email, internal_only, PublicKeyResponseCacheValue(response), UnixTimestamp::now()],
        )
        .await?;
        Ok(())
    }

    pub async fn get(
        email: String,
        internal_only: bool,
        tether: &Tether,
    ) -> Result<Option<PublickeyResponseCacheEntry>, StashError> {
        Ok(tether.query::<_,PublickeyResponseCacheEntry>("SELECT response, timestamp FROM public_address_key_response_cache WHERE email=? AND internal_only=?",
       params![email, internal_only]).await?.into_iter().next())
    }
}
