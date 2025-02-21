use std::sync::Arc;

use proton_core_common::datatypes::EncryptedPushNotification as RealEncryptedPushNotification;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::{
    DecryptableMailPushNotification, DecryptedMailPushNotification as RealDecryptedPushNotification,
};

use crate::{errors::ActionError, uniffi_async};

use super::MailSession;

/// Encrypted push notification
///
/// This notification is completely encrypted so that push servers
/// cannot know topic/sender/recipient
///
#[derive(Clone, Debug, uniffi::Record)]
pub struct EncryptedPushNotification {
    /// UID
    ///
    pub auth_id: String,
    /// Encrypted payload of the notification
    ///
    pub encrypted_message: String,
}

impl From<EncryptedPushNotification> for RealEncryptedPushNotification {
    fn from(value: EncryptedPushNotification) -> Self {
        Self {
            auth_id: value.auth_id.into(),
            encrypted_message: value.encrypted_message,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DecryptedPushNotification {
    // TODO (ET-2204): Obviously this is not the final datastructure
    Email,
    OpenUrl,
}

impl From<RealDecryptedPushNotification> for DecryptedPushNotification {
    fn from(value: RealDecryptedPushNotification) -> Self {
        match value {
            RealDecryptedPushNotification::Email {} => Self::Email,
            RealDecryptedPushNotification::OpenUrl {} => Self::OpenUrl,
        }
    }
}

#[uniffi_export]
pub async fn decrypt_push_notification(
    session: Arc<MailSession>,
    encrypted: EncryptedPushNotification,
) -> Result<DecryptedPushNotification, ActionError> {
    uniffi_async(async move {
        let ctx = session.ctx_arc();
        let real_encrypted = RealEncryptedPushNotification::from(encrypted);
        let real_decrypted = real_encrypted
            .into_decrypted_push_mail_notification(ctx)
            .await?;

        let decrypted = DecryptedPushNotification::from(real_decrypted);

        Ok::<_, RealProtonMailError>(decrypted)
    })
    .await
    .map_err(ActionError::from)
}
