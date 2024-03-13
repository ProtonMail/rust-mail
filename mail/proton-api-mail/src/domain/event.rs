use crate::domain::{
    Address, Conversation, ConversationId, Label, LabelId, MailSettings, MessageId, MessageMetadata,
};
use crate::domain::{ConversationCount, MessageCount};
use proton_api_core::domain::{EventAction, User, UserProductUsedSpace, UserSettings};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::exports::serde_repr::Deserialize_repr;

#[derive(Debug, Deserialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum MoreEvents {
    No = 0,
    Yes = 1,
}

proton_api_core::declare_event!(MailEvent, {
    messages: Option<Vec<MessageEvent>>,
    addresses: Option<Vec<Address>>,
    labels: Option<Vec<LabelEvent>>,
    user: Option<User>,
    used_space: Option<i64>,
    message_counts:Option<Vec<MessageCount>>,
    conversation_counts:Option<Vec<ConversationCount>>,
    product_used_space:Option<UserProductUsedSpace>,
    conversations: Option<Vec<ConversationEvent>>,
    user_settings: Option<UserSettings>,
    mail_settings: Option<MailSettings>
});

/// Event data related to a Message event.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageEvent {
    #[serde(rename = "ID")]
    pub id: MessageId,
    pub action: EventAction,
    pub message: Option<MessageMetadata>,
}

/// Event data related to a Label event
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct LabelEvent {
    #[serde(rename = "ID")]
    pub id: LabelId,
    pub action: EventAction,
    pub label: Option<Label>,
}

/// Event data related to a Conversation event.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationEvent {
    #[serde(rename = "ID")]
    pub id: ConversationId,
    pub action: EventAction,
    pub conversation: Option<Conversation>,
}
