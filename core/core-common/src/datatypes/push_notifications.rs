use proton_api_core::services::proton::common::AuthId;
use proton_crypto_account::{keys::UnlockedUserKeys, proton_crypto::crypto::PGPProviderSync};
use proton_crypto_notifications::{
    DecryptableNotification, NotificationError, PGPEncryptedNotification,
};
use serde::{Deserialize, Serialize};
use tracing::error;

pub use proton_crypto_notifications::DecryptedNotification;

/// Decrypted push notification
///
/// # Parameters
///
/// * `T` - your BU message format.
///
#[derive(Debug, Clone)]
pub struct DecryptedPushNotification<T> {
    /// Which account is recepient of the message
    ///
    pub auth_id: AuthId,
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
    pub auth_id: AuthId,
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
        pgp_provider: &P,
        user_keys: &UnlockedUserKeys<P>,
    ) -> Result<DecryptedPushNotification<O>, NotificationError>
    where
        P: PGPProviderSync,
        for<'de> O: Deserialize<'de>,
    {
        let notification = self
            .decrypt(pgp_provider, user_keys)
            .inspect_err(|e| error!("Failed to decrypt push notification: {e:?}"))?;

        Ok(DecryptedPushNotification {
            auth_id: self.auth_id,
            notification,
        })
    }
}
