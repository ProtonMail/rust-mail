use base64::{Engine, prelude::BASE64_STANDARD};
use proton_core_api::services::proton::SessionId;
use proton_crypto_account::{
    errors::KeySerializationError, keys::PGPDeviceKey, proton_crypto::crypto::PGPProviderSync,
};
use proton_crypto_notifications::{
    DecryptableNotification, NotificationError, PGPEncryptedNotification,
};
use secrecy::{ExposeSecret, Secret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::error;

pub use proton_crypto_notifications::DecryptedNotification;

use crate::os::{KeyChainEntryKind, StoreInKeyChain};

/// Device public key stored in the database
pub struct StoredDevicePublicKey(String);

impl AsRef<str> for StoredDevicePublicKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl From<String> for StoredDevicePublicKey {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for StoredDevicePublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Device key stored in the keychain
pub struct StoredDevicePrivateKey(Secret<Vec<u8>>);

impl AsRef<Secret<Vec<u8>>> for StoredDevicePrivateKey {
    fn as_ref(&self) -> &Secret<Vec<u8>> {
        &self.0
    }
}

impl StoredDevicePrivateKey {
    /// Takes raw bytes
    ///
    #[must_use]
    pub fn with_bytes(value: Vec<u8>) -> Self {
        Self(Secret::new(value))
    }

    /// Transforms it to `PGPDeviceKey`
    ///
    pub fn to_device_key<P>(
        &self,
        pgp: &P,
    ) -> Result<PGPDeviceKey<P::PrivateKey, P::PublicKey>, KeySerializationError>
    where
        P: PGPProviderSync,
    {
        let key_data = self.0.expose_secret();

        PGPDeviceKey::deserialize_from_secure_storage(pgp, key_data)
    }

    #[must_use]
    fn to_base64(&self) -> SecretString {
        let key = self.0.expose_secret();

        SecretString::new(BASE64_STANDARD.encode(key))
    }

    fn from_base64(val: &SecretString) -> Result<Self, base64::DecodeError> {
        let val = val.expose_secret();
        let bytes = BASE64_STANDARD.decode(val)?;

        Ok(Self::with_bytes(bytes))
    }
}

impl StoreInKeyChain for StoredDevicePrivateKey {
    fn kind() -> KeyChainEntryKind {
        KeyChainEntryKind::DeviceKey
    }

    fn from_stored_string(
        s: SecretString,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self::from_base64(&s)?)
    }

    fn to_stored_string(&self) -> SecretString {
        self.to_base64()
    }
}

/// Decrypted push notification
///
#[derive(Debug, Clone)]
pub struct DecryptedPushNotification<T> {
    /// Which account is recepient of the message
    ///
    pub session_id: SessionId,
    /// Decrypted notification.
    ///
    /// This notification is BU agnostic. You may want to deserialize the internal data further.
    ///
    pub notification: DecryptedNotification<T>,
}

/// Encrypted push notification. This notification is completely encrypted
/// so that Google/Apple servers cannot know topic/sender/recipient.
///
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedPushNotification {
    /// Which account is recepient of the message
    #[serde(rename = "UID")]
    pub session_id: SessionId,
    /// Message that is encrypted using PGP key.
    /// Not only the body of the message is encrypted, but metadata as well.
    pub encrypted_message: String,
}

impl PGPEncryptedNotification for EncryptedPushNotification {
    fn pgp_encrypted_notification_data(&self) -> &[u8] {
        self.encrypted_message.as_bytes()
    }
}

impl DecryptableNotification for EncryptedPushNotification {}

impl EncryptedPushNotification {
    /// Decrypt notification
    ///
    pub fn into_decrypted_push_notification<P, O>(
        self,
        pgp: &P,
        device_key: &PGPDeviceKey<P::PrivateKey, P::PublicKey>,
    ) -> Result<DecryptedPushNotification<O>, NotificationError>
    where
        P: PGPProviderSync,
        for<'de> O: Deserialize<'de>,
    {
        let notification = self
            .decrypt(pgp, device_key)
            .inspect_err(|e| error!("Failed to decrypt push notification: {e:?}"))?;

        Ok(DecryptedPushNotification {
            session_id: self.session_id,
            notification,
        })
    }
}
