use std::sync::Arc;

use proton_core_common::datatypes::EncryptedPushNotification as RealEncryptedPushNotification;
use proton_mail_common::datatypes::mail_notifications::{
    DecryptableInboxPushNotification,
    DecryptedEmailPushNotification as RealDecryptedEmailPushNotification,
    DecryptedInboxPushNotification as RealDecryptedInboxPushNotification,
    DecryptedOpenUrlPushNotification as RealDecryptedOpenUrlPushNotification,
    NotificationSender as RealNotificationSender,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

use crate::core::datatypes::RemoteId;
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

/// Decrypted notification usable only in the context of the Inbox application
///
#[derive(Clone, Debug, uniffi::Enum)]
pub enum DecryptedPushNotification {
    /// Decrypted notification that is pushed when user receives a new email.
    ///
    Email(DecryptedEmailPushNotification),
    /// Decrypted notification that is pushed when user logged in in the separate device.
    /// We use it to show webpage.
    ///
    OpenUrl(DecryptedOpenUrlPushNotification),
}

impl From<RealDecryptedInboxPushNotification> for DecryptedPushNotification {
    fn from(value: RealDecryptedInboxPushNotification) -> Self {
        match value {
            RealDecryptedInboxPushNotification::Email(email) => Self::Email(email.into()),
            RealDecryptedInboxPushNotification::OpenUrl(open_url) => Self::OpenUrl(open_url.into()),
        }
    }
}

/// Decrypted notification that is pushed when user receives a new email.
///
#[derive(Clone, Debug, uniffi::Record)]
pub struct DecryptedEmailPushNotification {
    /// The subject of the email message
    ///
    pub subject: String,

    /// Information about who sent the message
    ///
    pub sender: NotificationSender,

    /// Remote message ID
    ///
    pub message_id: RemoteId,
}

impl From<RealDecryptedEmailPushNotification> for DecryptedEmailPushNotification {
    fn from(value: RealDecryptedEmailPushNotification) -> Self {
        Self {
            subject: value.subject,
            sender: value.sender.into(),
            message_id: value.message_id.into(),
        }
    }
}

/// Decrypted notification that is pushed when user's device has to open a web page with given URL.
/// Used for example when user logs in in the new device
///
#[derive(Clone, Debug, uniffi::Record)]
pub struct DecryptedOpenUrlPushNotification {
    /// Content of the notification
    pub content: String,

    /// Information about who sent the notification
    pub sender: NotificationSender,

    /// URL
    pub url: String,
}

impl From<RealDecryptedOpenUrlPushNotification> for DecryptedOpenUrlPushNotification {
    fn from(value: RealDecryptedOpenUrlPushNotification) -> Self {
        Self {
            content: value.content,
            sender: value.sender.into(),
            url: value.url,
        }
    }
}

/// Who sent the notification
///
/// This data structure is very similar to [`super::datatypes::MessageSender`] but simplified
///
#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct NotificationSender {
    /// Name of the sender
    ///
    pub name: String,

    /// Email address of the sender
    ///
    pub address: String,

    /// Contact group of the sender
    ///
    pub group: String,
}

impl From<RealNotificationSender> for NotificationSender {
    fn from(value: RealNotificationSender) -> Self {
        Self {
            name: value.name,
            address: value.address,
            group: value.group,
        }
    }
}

/// Decrypt and deserialize Push notification.
/// This function is mail (inbox) specific
///
/// # Parameters
///
/// * `session` - a mail session, used before logging in. Based on the notification payload, the SDK will find
///   user session accordingly.
/// * `encrypted` - encrypted message received from the Push API
///
/// # Errors
///
/// This function may return an error if decryption fails, or it the decrypted message is not in the expected
/// format. It may also fail when saving new message to the database
///
#[uniffi_export]
pub async fn decrypt_push_notification(
    session: Arc<MailSession>,
    encrypted: EncryptedPushNotification,
) -> Result<DecryptedPushNotification, ActionError> {
    uniffi_async(async move {
        let ctx = session.ctx_arc();
        let real_encrypted = RealEncryptedPushNotification::from(encrypted);
        let real_decrypted = real_encrypted
            .try_into_decrypted_inbox_mail_notification(ctx)
            .await?;

        let decrypted = DecryptedPushNotification::from(real_decrypted);

        Ok::<_, RealProtonMailError>(decrypted)
    })
    .await
    .map_err(ActionError::from)
}
