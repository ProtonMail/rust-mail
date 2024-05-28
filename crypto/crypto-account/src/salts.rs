use std::fmt::Debug;

use crate::keys::KeyId;
use base64::{prelude::BASE64_STANDARD as BASE_64, Engine as _};
use proton_crypto::{srp::SRPProvider, CryptoError};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

// bcrypt prefix and salt (first 29 bytes)
const PREFIX_LEN: usize = 29;

#[derive(Debug, thiserror::Error)]
pub enum SaltError {
    #[error("Could not find key with id {0}")]
    KeyNotFound(KeyId),
    #[error("Key with id {0} has no salt value")]
    KeyHasNoSalt(KeyId),
    #[error("Could not decode key salt: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Failed to hash: {0}")]
    Hash(#[from] CryptoError),
    #[error("Failed to decoded hash")]
    HashDecode,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Salt {
    #[serde(rename = "ID")]
    pub id: KeyId,
    #[serde(rename = "KeySalt")]
    pub key_salt: Option<String>,
}

/// A hashed secret to decrypt a user key.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct KeySecret(Vec<u8>);

impl KeySecret {
    /// Creates a key secret from a byte vector.
    pub fn new(data: Vec<u8>) -> Self {
        KeySecret(data)
    }

    /// Returns a slice of the key in bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsRef<[u8]> for KeySecret {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Debug for KeySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeySecret: [CONFIDENTIAL]")
    }
}

#[derive(Deserialize, Debug)]
pub struct Salts(Vec<Salt>);
impl Salts {
    pub fn new(salts: impl IntoIterator<Item = Salt>) -> Self {
        Self(salts.into_iter().collect::<Vec<_>>())
    }

    pub fn salt_for_key<T: SRPProvider>(
        &self,
        srp_provider: &T,
        key: &KeyId,
        key_pass: &[u8],
    ) -> Result<KeySecret, SaltError> {
        let Some(salt) = self.0.iter().find(|&v| v.id == *key) else {
            return Err(SaltError::KeyNotFound(key.clone()));
        };

        let Some(key_salt) = &salt.key_salt else {
            return Err(SaltError::KeyHasNoSalt(key.clone()));
        };

        let key_salt_decoded = BASE_64.decode(key_salt)?;

        let result = srp_provider.mailbox_password(key_pass, key_salt_decoded)?;

        if result.as_ref().len() < PREFIX_LEN {
            return Err(SaltError::HashDecode);
        }

        // Remove bcrypt prefix and salt (first 29 characters)
        Ok(KeySecret(result.as_ref()[PREFIX_LEN..].as_ref().to_vec()))
    }
}
