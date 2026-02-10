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

#[derive(Clone, Debug, uniffi::Record)]
pub struct EncryptedPushNotification {
    pub session_id: String, // aka UID (not to be confused with user id!)
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

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DecryptedPushNotification {
    Email(DecryptedEmailPushNotification),
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

#[derive(Clone, Debug, uniffi::Record)]
pub struct DecryptedEmailPushNotification {
    pub subject: String,
    pub sender: NotificationSender,
    pub message_id: RemoteId,
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

// Note: This enum is available only on Android
#[derive(Clone, Debug, uniffi::Enum)]
pub enum DecryptedEmailPushNotificationAction {
    MessageCreated,
    MessageTouched,
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
    pub content: String,
    pub sender: NotificationSender,
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

#[derive(Clone, Debug, Default, Eq, PartialEq, uniffi::Record)]
pub struct NotificationSender {
    pub name: String,
    pub address: String,
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
    MarkAsRead { remote_id: RemoteId },
    MoveToArchive { remote_id: RemoteId },
    MoveToTrash { remote_id: RemoteId },
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
