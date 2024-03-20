use super::bool_from_integer;
use serde::{Deserialize, Serialize};
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

crate::string_id! {
    /// Represent a user's API key ID.
    KeyId
}

/// Represent a flag of a key containing a bit map.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Copy, Default)]
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

impl KeyFlag {
    /// Returns the key flag bitmap as u32.
    pub fn to_u32(&self) -> u32 {
        self.0
    }
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

crate::string_id! {
    ///
    KeyTokenSignature
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
#[serde(rename_all = "PascalCase")]
/// Represents a locked jey retrieved from the API.
pub struct LockedKey {
    #[serde(rename = "ID")]
    /// Proton ID of the key.
    pub id: KeyId,
    /// Proton version of the key.
    pub version: u32,
    /// OpenPGP private key armored.
    pub private_key: String,
    /// Token to decrypt a key via another key (e.g., user key).
    pub token: Option<String>,
    /// OpenPGP Signature to verify the token.
    pub signature: Option<String>, // Only available for address keys
    /// (Deprecated) Migrated accounts do not have the activation field set.
    pub activation: Option<String>,
    #[serde(deserialize_with = "bool_from_integer")]
    /// Is the key the primary key to use.
    pub primary: bool,
    #[serde(deserialize_with = "bool_from_integer")]
    /// The key is active and should be decryptable.
    pub active: bool,
    /// Key flags encoded in a bitmap.
    pub flags: Option<KeyFlag>, // Only available for address keys
    /// Secret for key recovery of a local file.
    pub recovery_secret: Option<String>, // Only available for user keys
    /// Signature for the recovery secret.
    pub recovery_secret_signature: Option<String>, // Only available for user keys
    #[serde(rename = "AddressForwardingID")]
    /// Represents a valid associated Address Forwarding instance, if not None.
    pub address_forwarding_id: Option<String>, // Only available for address keys
}

/// Represents a public key retrieved from the API.
///
/// For example the 'core/v4/keys/all' route can be used to retrieve public keys of
/// another proton user.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct APIPublicKey {
    /// Origin of the public key.
    pub source: APIPublicKeySource,
    /// Key flags encoded in a bitmap.
    pub flags: KeyFlag,
    /// OpenPGP armored public key.
    pub public_key: String,
}
