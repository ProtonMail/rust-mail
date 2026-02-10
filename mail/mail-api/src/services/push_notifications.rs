use super::proton::common::MessageId;
use proton_core_api::services::proton::{PrivateEmail, PrivateString};
use serde::Deserialize;
use serde_with::{BoolFromInt, serde_as};

/// Who sent the notification
///
/// This data structure is very similar to [`super::proton::prelude::MessageSender`] but simplified
///
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
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

/// Decrypted notification for Inbox application
///
/// This is an enum that contains all possible notifications for the Inbox app.
///
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecryptedInboxPushNotification {
    Email {
        data: DecryptedEmailPushNotification,
    },
    OpenUrl {
        data: DecryptedOpenUrlPushNotification,
    },
}

/// This is decrypted email notification. It is received whenever user gets a new mail message.
///
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedEmailPushNotification {
    /// A number rendered in the badge next to the icon.
    ///
    pub badge: u64,

    /// A subject of the message
    ///
    #[serde(rename = "body")]
    pub subject: String,

    /// This is hardcoded on the backend, always with the value "`large_icon`"
    ///
    pub large_icon: String,

    /// Remote Id of the incoming message
    ///
    pub message_id: MessageId,

    /// Who sent the message
    ///
    pub sender: NotificationSender,

    /// This is hardcoded on the backend, always with the value "`small_icon`"
    ///
    pub small_icon: String,

    /// Whether to play sound
    ///
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub sound: bool,

    /// TODO: Describe
    pub subtitle: String,

    /// TODO: Describe
    pub title: String,

    /// Whether to vibrate
    ///
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub vibrate: bool,

    /// What kind of action was made for this email
    /// Note: This field is available only on Android
    ///
    #[serde(default)]
    pub action: Option<DecryptedEmailPushNotificationAction>,
}

/// What kind of action was made for this email
/// Note: This enum is available only on Android
///
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    #[serde(untagged)]
    Unexpected(String),
}

/// This is decrypted notification that after clicking opens a web page.
///
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedOpenUrlPushNotification {
    /// TODO: Describe
    pub title: String,

    /// TODO: Describe
    pub subtitle: String,

    /// Content of the notification
    ///
    pub body: String,

    /// Who sent the message
    ///
    pub sender: NotificationSender,

    /// Whether to play sound
    ///
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub sound: bool,

    /// Whether to vibrate
    ///
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub vibrate: bool,

    /// TODO: Describe
    pub large_icon: String,

    /// TODO: Describe
    pub small_icon: String,

    /// A number rendered in the badge next to the icon.
    ///
    pub badge: u64,

    /// What website should be opened when user clicks the notification
    ///
    pub url: String,

    // This field is based on https://protonag.atlassian.net/wiki/spaces/INBOX/pages/46369569/Push+Notifications+in+Proton+Mail+iOS
    /// TODO: Describe
    pub message_id: String,
}
