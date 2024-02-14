use crate::domain::{Label, LabelId, Message, MessageId};
use proton_api_core::domain::EventAction;
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
    labels: Option<Vec<LabelEvent>>
});

/// Event data related to a Message event.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageEvent {
    #[serde(rename = "ID")]
    pub id: MessageId,
    pub action: EventAction,
    pub message: Option<Message>,
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
