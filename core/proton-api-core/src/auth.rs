use crate::services::proton::common::RemoteId;
use proton_crypto_account::salts::KeySecret;
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Deref;

/// Authentication session data.
#[derive(Clone)]
pub struct Auth {
    /// The authentication token for the current session.
    pub access_token: SecretString,

    /// The email address of the current user.
    pub email: String,

    /// A [`KeySecret`] to unlock the user's keys.
    pub key_secret: Option<UserKeySecret>,

    /// TODO: Document this field.
    pub refresh_token: SecretString,

    /// TODO: Document this field.
    pub scope: String,

    /// The UID of the current session.
    pub uid: RemoteId,

    /// The remote ID of the current user.
    pub user_id: RemoteId,
}

/// TODO: Document this struct.
#[derive(Debug, Clone)]
pub struct SecretString(RealSecretString);

impl Deref for SecretString {
    type Target = RealSecretString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SecretString(RealSecretString::deserialize(deserializer)?))
    }
}

impl Eq for SecretString {}

impl From<RealSecretString> for SecretString {
    fn from(value: RealSecretString) -> Self {
        Self(value)
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(RealSecretString::new(value))
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("[redacted]")
    }
}

/// The user key secret to unlock user keys.
#[derive(Debug, Clone)]
pub struct UserKeySecret(pub KeySecret);

impl UserKeySecret {
    /// Exposes the internal key secret to unlock user keys.
    #[must_use]
    pub fn expose_secret(&self) -> &KeySecret {
        &self.0
    }
}

impl<T: Into<Vec<u8>>> From<T> for UserKeySecret {
    fn from(value: T) -> Self {
        Self(KeySecret::new(value.into()))
    }
}
