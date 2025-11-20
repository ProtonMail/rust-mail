use std::sync::Arc;

use proton_core_common::datatypes::EncryptedPushNotification as RealEncryptedPushNotification;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::actions::notifications_quick_actions;
use proton_mail_common::datatypes::mail_notifications::{
    DecryptableInboxPushNotification,
    DecryptedEmailPushNotification as RealDecryptedEmailPushNotification,
    DecryptedEmailPushNotificationAction as RealDecryptedEmailPushNotificationAction,
    DecryptedInboxPushNotification as RealDecryptedInboxPushNotification,
    DecryptedOpenUrlPushNotification as RealDecryptedOpenUrlPushNotification,
    NotificationSender as RealNotificationSender,
    PushNotificationQuickAction as RealPushNotificationQuickAction,
};

use crate::core::datatypes::RemoteId;
use crate::core::{FFIKeyChain, OSKeyChain, StoredSession};
use crate::errors::VoidActionResult;
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
    pub session_id: String,
    /// Encrypted payload of the notification
    ///
    pub encrypted_message: String,
}

impl From<EncryptedPushNotification> for RealEncryptedPushNotification {
    fn from(value: EncryptedPushNotification) -> Self {
        Self {
            session_id: value.session_id.into(),
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

    /// What kind of action was made for this email
    /// Note: This field is available only on Android
    ///
    pub action: Option<DecryptedEmailPushNotificationAction>,
}

impl From<RealDecryptedEmailPushNotification> for DecryptedEmailPushNotification {
    fn from(value: RealDecryptedEmailPushNotification) -> Self {
        Self {
            subject: value.subject,
            sender: value.sender.into(),
            message_id: value.message_id.into(),
            action: value.action.map(From::from),
        }
    }
}

/// What kind of action was made for this email
/// Note: This enum is available only on Android
///
#[derive(Clone, Debug, uniffi::Enum)]
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

impl From<RealDecryptedEmailPushNotificationAction> for DecryptedEmailPushNotificationAction {
    fn from(value: RealDecryptedEmailPushNotificationAction) -> Self {
        match value {
            RealDecryptedEmailPushNotificationAction::MessageCreated => Self::MessageCreated,
            RealDecryptedEmailPushNotificationAction::MessageTouched => Self::MessageTouched,
            RealDecryptedEmailPushNotificationAction::Unexpected(action) => {
                Self::Unexpected(action)
            }
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
            name: value.name.into_clear_text_string(),
            address: value.address.into_clear_text_string(),
            group: value.group.into_clear_text_string(),
        }
    }
}

/// Decrypt and deserialize Push notification.
/// This function is mail (inbox) specific
///
/// # Errors
///
/// This function may return an error if decryption fails, or it the decrypted message is not in the expected
/// format. It may also fail when saving new message to the database
///
#[uniffi_export]
pub async fn decrypt_push_notification(
    key_chain: Box<dyn OSKeyChain>,
    encrypted: EncryptedPushNotification,
) -> Result<DecryptedPushNotification, ActionError> {
    uniffi_async(async move {
        let real_encrypted = RealEncryptedPushNotification::from(encrypted);
        let real_decrypted = real_encrypted
            .try_into_decrypted_inbox_mail_notification(Arc::new(FFIKeyChain::from(key_chain)))
            .await?;

        let decrypted = DecryptedPushNotification::from(real_decrypted);

        Ok::<_, RealProtonMailError>(decrypted)
    })
    .await
    .map_err(ActionError::from)
}

/// Quick actions available for mail related push notifications.
/// It operates on remote ids since local ids are unknown at this point.
///
#[derive(Debug, uniffi::Enum)]
pub enum PushNotificationQuickAction {
    /// Marks email (being a subject of this notification) as "Read".
    /// It might be no-op if user managed to mark it on another device
    /// (It does not act as "toggle").
    MarkAsRead {
        /// Remote id of the message.
        remote_id: RemoteId,
    },

    /// Moves email (being a subject of this notification) to "Archive" folder.
    MoveToArchive {
        /// Remote id of the message.
        remote_id: RemoteId,
    },

    /// Moves email (being a subject of this notification) to "Trash" folder.
    MoveToTrash {
        /// Remote id of the message.
        remote_id: RemoteId,
    },
}

impl From<PushNotificationQuickAction> for RealPushNotificationQuickAction {
    fn from(value: PushNotificationQuickAction) -> Self {
        match value {
            PushNotificationQuickAction::MarkAsRead { remote_id } => Self::MarkAsRead {
                remote_id: remote_id.into(),
            },
            PushNotificationQuickAction::MoveToArchive { remote_id } => Self::MoveToArchive {
                remote_id: remote_id.into(),
            },
            PushNotificationQuickAction::MoveToTrash { remote_id } => Self::MoveToTrash {
                remote_id: remote_id.into(),
            },
        }
    }
}

#[uniffi_export]
impl MailSession {
    /// Insert the quick action into the queue and execute local part immediately.
    ///
    #[returns(VoidActionResult)]
    pub async fn execute_notification_quick_action(
        &self,
        session: Arc<StoredSession>,
        action: PushNotificationQuickAction,
        time_left_ms: Option<u64>,
    ) -> Result<(), ActionError> {
        let mail_ctx = self.ctx_arc();

        uniffi_async(async move {
            notifications_quick_actions::exec(
                mail_ctx,
                session.session(),
                action.into(),
                time_left_ms,
            )
            .await
            .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ActionError::from)
        .into()
    }
}
