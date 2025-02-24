use serde::Deserialize;
use serde_with::{serde_as, BoolFromInt};

use super::proton::{common::MessageId, prelude::MessageSender};

/// Decrypted notification for Inbox application
///
/// This is an enum that contains all possible notifications for the Inbox app.
///
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // ET-2204: Will go away when OpenUrl is properly implemented
pub enum DecryptedInboxPushNotification {
    Email {
        data: DecryptedEmailPushNotification,
    },
    OpenUrl {
        // TODO (ET-2204): Replace with proper structure
        data: serde_json::Value,
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
    pub body: String,

    /// TODO: Describe
    pub large_icon: String,

    /// Remote Id of the incoming message
    ///
    pub message_id: MessageId,

    /// Who sent the message
    ///
    pub sender: MessageSender,

    /// TODO: Describe
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
}
