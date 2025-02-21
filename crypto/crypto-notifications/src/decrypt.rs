use proton_crypto_account::proton_crypto::{
    crypto::{DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData},
    CryptoError,
};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Failed to decrypt the notification: {0}")]
    Decryption(CryptoError),

    #[error("Failed to deserialize notification: {0}")]
    Deserialization(serde_json::Error),
}

/// Notification stored as generic JSON object.
/// Because push notifications are BU independent, we do not assume its content at the
/// decryption stage
///
/// # Parameters
///
/// * `T` - your BU message format.
///
#[derive(Deserialize, Clone, Debug)]
#[serde(transparent)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct DecryptedNotification<T> {
    pub inner: T,
}

/// Notification encrypted by PGP key. Can be decrypted by [`DecryptableNotification::decrypt`].
///
pub trait GettablePGPNotification {
    /// Encrypted slice of bytes
    ///
    fn pgp_notification(&self) -> &[u8];
}

pub trait DecryptableNotification: GettablePGPNotification {
    /// Decrypt the notification
    ///
    /// Note, that function does not verify notification, nor it provides verifier.
    /// It is because we are receiving notification encrypted with **public** key.
    ///
    fn decrypt<T, O>(
        &self,
        pgp_provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
    ) -> Result<DecryptedNotification<O>, NotificationError>
    where
        T: PGPProviderSync,
        for<'de> O: Deserialize<'de>,
    {
        let data = self.pgp_notification();
        let decrypted_notification = pgp_provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt(data, DataEncoding::Armor)
            .map_err(NotificationError::Decryption)?;

        let raw_notification_data = decrypted_notification.into_vec();
        let notification = serde_json::from_slice(&raw_notification_data)
            .map_err(NotificationError::Deserialization)?;

        Ok(notification)
    }
}
