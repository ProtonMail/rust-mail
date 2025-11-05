#![allow(async_fn_in_trait)]
//! This module contains mail specific push notifications.
//!
//! It's using shared base from [`proton_core_common`] but with the context of mail application
//!

use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::push_notifications::DecryptedEmailPushNotificationAction as ApiDecryptedEmailPushNotificationAction;
use proton_mail_api::services::push_notifications::DecryptedInboxPushNotification as ApiDecryptedInboxPushNotification;
use proton_mail_api::services::push_notifications::NotificationSender as ApiNotificationSender;

use crate::MailContextError;
use proton_core_api::services::proton::{PrivateEmail, PrivateString};
use proton_core_common::datatypes::EncryptedPushNotification;
use proton_core_common::datatypes::StoredDevicePrivateKey;
use proton_core_common::os::KeyChain;
use proton_core_common::os::KeyChainExt;
use proton_crypto_account::keys::PGPDeviceKey;
use proton_crypto_account::proton_crypto;
use secrecy::ExposeSecret;
use serde_with::serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;

/// Quick actions available for mail related push notifications.
/// It operates on remote ids since local ids are unknown at this point.
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PushNotificationQuickAction {
    MarkAsRead { remote_id: MessageId },
    MoveToArchive { remote_id: MessageId },
    MoveToTrash { remote_id: MessageId },
}

/// Decrypted notification usable only in the context of the Inbox application
///
#[derive(Clone, Debug)]
pub enum DecryptedInboxPushNotification {
    /// Decrypted notification that is pushed when user receives a new email.
    ///
    Email(DecryptedEmailPushNotification),
    /// Decrypted notification that is pushed when user logged in in the separate device.
    /// We use it to show webpage.
    ///
    OpenUrl(DecryptedOpenUrlPushNotification),
}

impl From<ApiDecryptedInboxPushNotification> for DecryptedInboxPushNotification {
    fn from(value: ApiDecryptedInboxPushNotification) -> Self {
        match value {
            ApiDecryptedInboxPushNotification::Email { data } => {
                Self::Email(DecryptedEmailPushNotification {
                    subject: data.subject,
                    sender: data.sender.into(),
                    message_id: data.message_id,
                    action: data.action.map(From::from),
                })
            }
            ApiDecryptedInboxPushNotification::OpenUrl { data } => {
                Self::OpenUrl(DecryptedOpenUrlPushNotification {
                    content: data.body,
                    sender: data.sender.into(),
                    url: data.url,
                })
            }
        }
    }
}

/// Decrypted notification that is pushed when user receives a new email.
///
#[derive(Clone, Debug)]
pub struct DecryptedEmailPushNotification {
    /// The subject of the email message
    ///
    pub subject: String,

    /// Information about who sent the message
    ///
    pub sender: NotificationSender,

    /// Remote message ID
    ///
    pub message_id: MessageId,

    /// What kind of action was made for this email
    /// Note: This field is available only on Android
    ///
    pub action: Option<DecryptedEmailPushNotificationAction>,
}

/// What kind of action was made for this email
/// Note: This enum is available only on Android
///
#[derive(Clone, Debug)]
pub enum DecryptedEmailPushNotificationAction {
    /// Message has been created. It requires a full notification
    ///
    MessageCreated,
    /// Message has been touched on another device. We want to hide
    /// notification
    ///
    MessageTouched,

    /// Unexpected action
    ///
    Unexpected(String),
}

impl From<ApiDecryptedEmailPushNotificationAction> for DecryptedEmailPushNotificationAction {
    fn from(value: ApiDecryptedEmailPushNotificationAction) -> Self {
        match value {
            ApiDecryptedEmailPushNotificationAction::MessageCreated => Self::MessageCreated,
            ApiDecryptedEmailPushNotificationAction::MessageTouched => Self::MessageTouched,
            ApiDecryptedEmailPushNotificationAction::Unexpected(action) => Self::Unexpected(action),
        }
    }
}

/// Decrypted notification that is pushed for example when user logs in on a separate device.
/// Clicking on such a notification opens URL in a webview.
///
#[derive(Clone, Debug)]
pub struct DecryptedOpenUrlPushNotification {
    /// Content of the notification
    pub content: String,

    /// Information about who sent the notification
    pub sender: NotificationSender,

    /// URL
    pub url: String,
}

/// Who sent the notification
///
/// This data structure is very similar to [`super::MessageSender`] but simplified
///
#[derive(Clone, Debug)]
pub struct NotificationSender {
    /// Name of the sender
    ///
    pub name: PrivateString,

    /// Email address of the sender
    ///
    pub address: PrivateEmail,

    /// Contact group of the sender
    ///
    pub group: PrivateString,
}

impl From<ApiNotificationSender> for NotificationSender {
    fn from(value: ApiNotificationSender) -> Self {
        Self {
            name: value.name,
            address: value.address,
            group: value.group,
        }
    }
}

/// Notification specific for the Inbox, that can be decrypted and deserialized
///
pub trait DecryptableInboxPushNotification {
    /// Decrypt and deserialize generic push notification into Inbox-specific notification
    ///
    async fn try_into_decrypted_inbox_mail_notification(
        self,
        key_chain: Arc<dyn KeyChain>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError>;
}

impl DecryptableInboxPushNotification for EncryptedPushNotification {
    #[tracing::instrument(skip_all)]
    async fn try_into_decrypted_inbox_mail_notification(
        self,
        key_chain: Arc<dyn KeyChain>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError> {
        let pgp = proton_crypto::new_pgp_provider();

        let Some(key) = key_chain.load::<StoredDevicePrivateKey>()? else {
            error!("Missing device decryption key in the keychain");
            return Err(MailContextError::Crypto);
        };

        let pgp_device_key = PGPDeviceKey::deserialize_from_secure_storage(
            &pgp,
            key.as_ref().expose_secret().as_slice(),
        )
        .map_err(|_e| {
            error!("Could not load device key");
            MailContextError::Crypto
        })?;

        let decrypted_notification = self
            .into_decrypted_push_notification(&pgp, &pgp_device_key)
            .inspect_err(|e| error!("Failed to decrypt mail notification: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        let decrypted_mail_notification: ApiDecryptedInboxPushNotification =
            decrypted_notification.notification.inner;

        let decrypted_mail_notification =
            DecryptedInboxPushNotification::from(decrypted_mail_notification);

        Ok(decrypted_mail_notification)
    }
}
