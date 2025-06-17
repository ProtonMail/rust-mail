use proton_crypto_account::proton_crypto::{
    CryptoError,
    crypto::{DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData},
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
#[derive(Deserialize, Clone, Debug)]
#[serde(transparent)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct DecryptedNotification<T> {
    pub inner: T,
}

/// Notification encrypted by PGP key. Can be decrypted by [`DecryptableNotification::decrypt`].
///
pub trait PGPEncryptedNotification {
    /// Encrypted slice of bytes
    ///
    fn pgp_encrypted_notification_data(&self) -> &[u8];
}

pub trait DecryptableNotification: PGPEncryptedNotification {
    /// Decrypt the notification
    ///
    /// Note, that function does not verify notification, nor it provides verifier.
    /// It is because we are receiving notification encrypted with **public** key.
    ///
    fn decrypt<P, O>(
        &self,
        pgp: &P,
        decryption_key: &impl AsRef<P::PrivateKey>,
    ) -> Result<DecryptedNotification<O>, NotificationError>
    where
        P: PGPProviderSync,
        for<'de> O: Deserialize<'de>,
    {
        let data = self.pgp_encrypted_notification_data();
        let decrypted_notification = pgp
            .new_decryptor()
            .with_decryption_key(decryption_key.as_ref())
            .decrypt(data, DataEncoding::Armor)
            .map_err(NotificationError::Decryption)?;

        let raw_notification_data = decrypted_notification.into_vec();
        let notification = serde_json::from_slice(&raw_notification_data)
            .map_err(NotificationError::Deserialization)?;

        Ok(notification)
    }
}
