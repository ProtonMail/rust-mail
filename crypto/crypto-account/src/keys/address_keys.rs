use std::ops::Deref;

use futures::future::join_all;

use crate::{
    crypto::{
        generate_locked_pgp_key_with_token, generate_token_values, unlock_legacy_key,
        unlock_legacy_key_async,
    },
    errors::AccountCryptoError,
    salts::KeySecret,
};

use super::{
    ArmoredPrivateKey, DecryptedUserKey, EncryptedKeyToken, KeyError, KeyFlag, KeyId,
    KeyTokenSignature, LockedKey, UnlockResult, UnlockedUserKey,
};
use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, KeyGeneratorAlgorithm, PGPProviderAsync, PGPProviderSync,
    PrivateKey, PublicKey,
};
use serde::{Deserialize, Serialize};

#[allow(type_alias_bounds)]
pub type UnlockedAddressKey<Provider: PGPProviderSync> =
    DecryptedAddressKey<<Provider>::PrivateKey, <Provider>::PublicKey>;

/// Represents the unlocked address keys associated with a user's email address.
///
/// Provides utility methods for selecting and managing these keys.
#[allow(clippy::module_name_repetitions)]
pub struct UnlockedAddressKeys<Provider: PGPProviderSync>(Vec<UnlockedAddressKey<Provider>>);

impl<Provider: PGPProviderSync> Deref for UnlockedAddressKeys<Provider> {
    type Target = Vec<UnlockedAddressKey<Provider>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Provider: PGPProviderSync> AsRef<Vec<UnlockedAddressKey<Provider>>>
    for UnlockedAddressKeys<Provider>
{
    fn as_ref(&self) -> &Vec<UnlockedAddressKey<Provider>> {
        &self.0
    }
}

impl<Provider: PGPProviderSync> AsRef<[UnlockedAddressKey<Provider>]>
    for UnlockedAddressKeys<Provider>
{
    fn as_ref(&self) -> &[UnlockedAddressKey<Provider>] {
        &self.0
    }
}

impl<Provider: PGPProviderSync> AsMut<Vec<UnlockedAddressKey<Provider>>>
    for UnlockedAddressKeys<Provider>
{
    fn as_mut(&mut self) -> &mut Vec<UnlockedAddressKey<Provider>> {
        &mut self.0
    }
}

impl<Provider: PGPProviderSync> AsMut<[UnlockedAddressKey<Provider>]>
    for UnlockedAddressKeys<Provider>
{
    fn as_mut(&mut self) -> &mut [UnlockedAddressKey<Provider>] {
        &mut self.0
    }
}

impl<Provider: PGPProviderSync> From<Vec<UnlockedAddressKey<Provider>>>
    for UnlockedAddressKeys<Provider>
{
    fn from(value: Vec<UnlockedAddressKey<Provider>>) -> Self {
        Self(value)
    }
}

impl<Provider: PGPProviderSync> Clone for UnlockedAddressKeys<Provider> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Provider: PGPProviderSync> UnlockedAddressKeys<Provider> {
    /// Retrieves the primary address key for encryption and signing operations.
    pub fn primary(&self) -> Option<&UnlockedAddressKey<Provider>> {
        // For now we treat the first key in the list as primary.
        // - This will change with OpenPGP v6 and PQC keys, where
        //   multiple primary address keys are present.
        // - Key transparency
        self.0.first()
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
    /// Creates new `AddressKeys` from an iterator of locked keys from the API.
    pub fn new(v: impl IntoIterator<Item = LockedKey>) -> Self {
        Self(Vec::from_iter(v))
    }
    /// Decrypts the address keys with the provided user keys.
    ///
    /// Returns the address keys that were successfully decrypted and verified using the provided user keys.
    /// If decryption or verification fails for a key, the key is not included in the returned vector.
    /// To be able to unlock legacy address keys a `passphrase` must also be provided.
    pub fn unlock<T: PGPProviderSync>(
        &self,
        provider: &T,
        user_keys: impl AsRef<[DecryptedUserKey<T::PrivateKey, T::PublicKey>]>,
        passphrase: Option<&KeySecret>,
    ) -> UnlockResult<UnlockedAddressKey<T>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_address_keys: Vec<UnlockedAddressKey<T>> =
            Vec::with_capacity(self.0.len());
        decrypted_address_keys.extend(self.0.iter().filter_map(|locked_key| {
            let Some(flags) = &locked_key.flags else {
                failed_keys.push(KeyError::MissingValue(locked_key.id.clone()));
                return None;
            };
            let (Some(token), Some(signature)) = (&locked_key.token, &locked_key.signature) else {
                // Try legacy decryption
                return match unlock_legacy_key(provider, locked_key, passphrase) {
                    Ok(unlocked_key) => Some(unlocked_key),
                    Err(err) => {
                        failed_keys.push(err);
                        return None;
                    }
                };
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
                primary: locked_key.primary,
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
    pub async fn unlock_async<T: PGPProviderAsync>(
        &self,
        provider: &T,
        user_keys: impl AsRef<[UnlockedAddressKey<T>]>,
        passphrase: Option<&KeySecret>,
    ) -> UnlockResult<UnlockedAddressKey<T>> {
        let mut failed_keys = Vec::new();
        let mut decrypted_address_keys: Vec<UnlockedAddressKey<T>> =
            Vec::with_capacity(self.0.len());
        let mut decrypted_address_key_futures: Vec<_> = Vec::with_capacity(self.0.len());
        for locked_key in &self.0 {
            decrypted_address_key_futures.push(async {
                let Some(flags) = &locked_key.flags else {
                    return Err(KeyError::MissingValue(locked_key.id.clone()));
                };
                let (Some(token), Some(signature)) = (&locked_key.token, &locked_key.signature)
                else {
                    // Try legacy decryption
                    return unlock_legacy_key_async(provider, locked_key, passphrase).await;
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

    /// Indicates whether any legacy address keys are present.
    ///
    /// Legacy means that the address key is encrypted with the same key secret
    /// as the user key. Thus, it does not contain an encrypted token or a token signature.
    pub fn contains_legacy_keys(&self) -> bool {
        self.0
            .iter()
            .any(|locked_key| locked_key.token.is_none() || locked_key.signature.is_none())
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

/// Represents a locked address key locally generated but not yet synced with the backend.
pub struct LocalAddressKey {
    /// The locked armored private key.
    pub private_key: ArmoredPrivateKey,
    /// Token to decrypt a key via another key (e.g., user key).
    ///
    /// Legacy keys do not have a token but are rather encrypted with the password derived key secret.
    pub token: Option<EncryptedKeyToken>,
    /// `OpenPGP` Signature to verify the token.
    ///
    /// Legacy keys do not have a token and, thus, no signature.
    pub signature: Option<KeyTokenSignature>,
    /// Address key flags
    pub flags: KeyFlag,
    /// Flag to indicate if this address key is the primary address key.
    pub primary: bool,
}

impl LocalAddressKey {
    /// Indicates whether this local address key is legacy.
    ///
    /// Legacy means that the address key is encrypted with the same key secret
    /// as the user key. Thus, it does not contain an encrypted token and a token signature.
    pub fn is_legacy(&self) -> bool {
        self.token.is_none() || self.signature.is_none()
    }

    /// Returns the token value (i.e., token and signature) of this local address key.
    ///
    /// # Errors
    /// Returns a [`AccountCryptoError::UnexpectedLegacy`] if this local address key does not contain
    /// an encrypted token and a signature.
    pub fn token(&self) -> Result<(&EncryptedKeyToken, &KeyTokenSignature), AccountCryptoError> {
        let (Some(enc_token), Some(token_signature)) = (&self.token, &self.signature) else {
            return Err(AccountCryptoError::UnexpectedLegacy);
        };
        Ok((enc_token, token_signature))
    }

    /// Generates a fresh user key and locks it with the provided user key.
    ///
    /// To use the default key algorithm for the generated key, call with [`KeyGeneratorAlgorithm::default()`].
    pub fn generate<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        email: &str,
        algorithm: KeyGeneratorAlgorithm,
        flags: KeyFlag,
        primary: bool,
        user_key: &UnlockedUserKey<Provider>,
    ) -> Result<Self, AccountCryptoError> {
        generate_locked_pgp_key_with_token(pgp_provider, email, email, algorithm, user_key, None)
            .map(|(private_key, token, signature)| LocalAddressKey {
                private_key,
                token: Some(token),
                signature: Some(signature),
                flags,
                primary,
            })
    }

    /// Locks an existing unlocked address key with a new user key.
    pub fn relock_address_key<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        unlocked_address_key: &UnlockedAddressKey<Provider>,
        parent_key: &UnlockedUserKey<Provider>,
    ) -> Result<Self, AccountCryptoError> {
        let (passphrase, token, signature) = generate_token_values(pgp_provider, parent_key, None)?;
        let private_key = pgp_provider
            .private_key_export(
                &unlocked_address_key.private_key,
                passphrase.as_bytes(),
                DataEncoding::Armor,
            )
            .map(|key_bytes| String::from_utf8(key_bytes.as_ref().to_vec()))
            .map_err(|_err| AccountCryptoError::GenerateKeyArmor)? // For the CryptoError error
            .map_err(|_err| AccountCryptoError::GenerateKeyArmor) // For the FromUtf8 error
            .map(ArmoredPrivateKey)?;
        Ok(Self {
            private_key,
            token: Some(token),
            signature: Some(signature),
            flags: unlocked_address_key.flags,
            primary: unlocked_address_key.primary,
        })
    }

    /// Locks an existing unlocked address key with a new key secret in legacy mode.
    ///
    /// Only use this method if a legacy key should be produced.
    /// In most scenarios this is not the case!
    pub fn relock_address_key_legacy<Provider: PGPProviderSync>(
        pgp_provider: &Provider,
        unlocked_address_key: &DecryptedAddressKey<<Provider>::PrivateKey, <Provider>::PublicKey>,
        salted_password: &KeySecret,
    ) -> Result<Self, AccountCryptoError> {
        let private_key = pgp_provider
            .private_key_export(
                &unlocked_address_key.private_key,
                salted_password,
                DataEncoding::Armor,
            )
            .map(|key_bytes| String::from_utf8(key_bytes.as_ref().to_vec()))
            .map_err(|_err| AccountCryptoError::GenerateKeyArmor)? // For the CryptoError error
            .map_err(|_err| AccountCryptoError::GenerateKeyArmor) // For the FromUtf8 error
            .map(ArmoredPrivateKey)?;
        Ok(Self {
            private_key,
            token: None,
            signature: None,
            flags: unlocked_address_key.flags,
            primary: unlocked_address_key.primary,
        })
    }

    /// Unlocks the locally generated address key with the provided user key.
    ///
    /// The key id is retrieved from the API upon registering the key.
    pub fn unlock_and_assign_key_id<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        key_id: KeyId,
        user_key: &UnlockedUserKey<Provider>,
    ) -> Result<UnlockedAddressKey<Provider>, AccountCryptoError> {
        let (token, signature) = self.token()?;
        let (private_key, public_key) = crate::crypto::import_key_with_token(
            pgp_provider,
            &self.private_key,
            token,
            signature,
            &[user_key],
            &[user_key],
            None,
        )?;
        Ok(DecryptedAddressKey {
            id: key_id,
            flags: self.flags,
            primary: self.primary,
            private_key,
            public_key,
        })
    }

    /// LEGACY: Unlocks the locally generated address key with the provided secret.
    ///
    /// The key id is retrieved from the API upon registering the key.
    /// Legacy means that the address key is encrypted with the same key secret
    /// as the user key. Thus, it does not contain an encrypted token and a token signature.
    pub fn unlock_legacy_and_assign_key_id<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        key_id: KeyId,
        key_secret: &KeySecret,
    ) -> Result<UnlockedAddressKey<Provider>, AccountCryptoError> {
        let private_key = pgp_provider
            .private_key_import(
                self.private_key.0.as_bytes(),
                key_secret,
                DataEncoding::Armor,
            )
            .map_err(AccountCryptoError::KeyImport)?;
        let public_key = pgp_provider
            .private_key_to_public_key(&private_key)
            .map_err(AccountCryptoError::KeyImport)?;
        Ok(DecryptedAddressKey {
            id: key_id,
            flags: self.flags,
            primary: self.primary,
            private_key,
            public_key,
        })
    }
}
