use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{Display, Formatter};

use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{FLAG_EMAIL_NO_ENCRYPT, FLAG_EMAIL_NO_SIGN, FLAG_NOT_COMPROMISED, FLAG_NOT_OBSOLETE};

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Eq, Debug, Hash, Clone, Copy)]
#[repr(u32)]
pub enum APIPublicKeySource {
    Proton = 1,
    WKD = 2,
    KOO = 3,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Could not decode source of the api public key: {0}")]
    SourceDecode(u32),
}

/// Represent a user's API key ID.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct KeyId(String);

impl Display for KeyId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<String>> From<T> for KeyId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for KeyId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Represent key flags
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Copy)]
pub struct KeyFlag(u32);

impl Display for KeyFlag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<u32>> From<T> for KeyFlag {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<u32> for KeyFlag {
    fn as_ref(&self) -> &u32 {
        &self.0
    }
}

impl KeyFlag {
    /// Returns true if the flag indicates no email signing.
    pub fn is_email_no_sign(&self) -> bool {
        (self.0 & FLAG_EMAIL_NO_SIGN) > 0
    }
    /// Returns true if the flag indicates no email encryption.
    ///
    /// If true the associated key can't be used to encrypt email.
    /// There are multiple scenarios where this can happen
    /// - the key is associated to a product without Mail, like Drive or VPN
    /// - the key is associated to an external address
    /// - the key is associated to an internal address e2e encryption disabled (e.g. because of forwarding)
    pub fn is_email_no_encryption(&self) -> bool {
        (self.0 & FLAG_EMAIL_NO_ENCRYPT) > 0
    }
    /// Returns true if the flag indicates that the associated key is obsolete.
    pub fn is_obsolete(&self) -> bool {
        (self.0 & FLAG_NOT_OBSOLETE) == 0
    }
    /// Returns true if the flag indicates that the associated key is compromised.
    pub fn is_compromised(&self) -> bool {
        (self.0 & FLAG_NOT_COMPROMISED) == 0
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LockedKey {
    #[serde(rename = "ID")]
    pub id: KeyId,
    pub version: u32,
    pub private_key: String,
    pub token: Option<String>,
    pub signature: Option<String>, // Only available for address keys
    pub activation: Option<String>,
    #[serde(deserialize_with = "bool_from_integer")]
    pub primary: bool,
    #[serde(deserialize_with = "bool_from_integer")]
    pub active: bool,
    pub flags: Option<u32>,              // Only available for address keys
    pub recovery_secret: Option<String>, // Only available for user keys
    pub recovery_secret_signature: Option<String>, // Only available for user keys
    #[serde(rename = "AddressForwardingID")]
    pub address_forwarding_id: Option<String>, // Only available for address keys
}

/// Represents a public key retrieved from the API.
///
/// For example the 'core/v4/keys/all' route can be used to retrieve public keys of
/// another proton user.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct APIPublicKey {
    pub source: APIPublicKeySource,
    pub flags: KeyFlag,
    pub public_key: String,
}

/// Deserialize bool from integer
fn bool_from_integer<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    if i64::deserialize(deserializer)? == 0_i64 {
        Ok(false)
    } else {
        Ok(true)
    }
}
