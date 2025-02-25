#![allow(async_fn_in_trait)]
//! This module contains mail specific push notifications.
//!
//! It's using shared base from [`proton_core_common`] but with the context of mail application
//!

use proton_api_mail::services::push_notifications::DecryptedInboxPushNotification as ApiDecryptedInboxPushNotification;
use proton_api_mail::services::push_notifications::NotificationSender as ApiNotificationSender;

use proton_core_common::datatypes::EncryptedPushNotification;
use proton_crypto_account::proton_crypto;
use proton_mail_ids::LocalMessageId;
use std::sync::Arc;
use tracing::error;

use crate::{models::Message, MailContext, MailContextError, MailUserContext};

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

impl DecryptedInboxPushNotification {
    /// Sync the message.
    ///
    /// Notification does not contain all message metadata that is necessary for us
    /// to save the message in our SQLite database.
    ///
    /// In order to make it happen (and we need to make it happen, because mobile applications are operating on local ids, not remote ids),
    /// we need to fetch missing info from API and then store it in our local cache.
    ///
    /// # Parameters
    ///
    /// * `ctx` - mail user context as we save in user specific DB
    /// * `push_notification` - payload received from the push notification
    ///
    /// # Returns
    ///
    /// Decrypted notification that contains local IDs and refers to models stored in our database
    ///
    /// # Errors
    ///
    /// This function may return an error in case of API error or when Stash fails to write to the database
    ///
    pub async fn sync(
        ctx: Arc<MailUserContext>,
        push_notification: ApiDecryptedInboxPushNotification,
    ) -> Result<Self, MailContextError> {
        match push_notification {
            ApiDecryptedInboxPushNotification::Email { data } => {
                let remote_message_id = data.message_id.clone();
                let (message, _) =
                    Message::force_sync_message_and_body(ctx, remote_message_id, false).await?;

                Ok(Self::Email(DecryptedEmailPushNotification {
                    subject: data.subject,
                    sender: data.sender.into(),
                    message_id: message.local_id.expect("Local ID"),
                }))
            }
            ApiDecryptedInboxPushNotification::OpenUrl { data } => {
                Ok(Self::OpenUrl(DecryptedOpenUrlPushNotification {
                    content: data.body,
                    sender: data.sender.into(),
                    url: data.url,
                }))
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

    /// Local message ID
    ///
    pub message_id: LocalMessageId,
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
    pub name: String,

    /// Email address of the sender
    ///
    pub address: String,

    /// TODO: Describe
    ///
    pub group: String,
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
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError>;
}

impl DecryptableInboxPushNotification for EncryptedPushNotification {
    async fn try_into_decrypted_inbox_mail_notification(
        self,
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError> {
        let pgp_provider = proton_crypto::new_pgp_provider();

        let auth_id = &self.auth_id;
        let Some(session) = ctx.get_session(auth_id.clone()).await? else {
            error!("Could not find a session with id {auth_id}");
            return Err(MailContextError::SessionMissing(auth_id.clone()));
        };
        let ctx = ctx.user_context_from_session(&session, None).await?;
        let tether = ctx.user_stash().connection();
        let user_keys = ctx.unlocked_user_keys(&pgp_provider, &tether).await?;

        let decrypted_notification = self
            .into_decrypted_push_notification(&pgp_provider, &user_keys)
            .inspect_err(|e| error!("Failed to decrypt mail notification: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        let decrypted_mail_notification: ApiDecryptedInboxPushNotification =
            decrypted_notification.notification.inner;

        tracing::warn!("Decrypted: {decrypted_mail_notification:#?}");

        let decrypted_mail_notification =
            DecryptedInboxPushNotification::sync(ctx.clone(), decrypted_mail_notification).await?;

        Ok(decrypted_mail_notification)
    }
}
