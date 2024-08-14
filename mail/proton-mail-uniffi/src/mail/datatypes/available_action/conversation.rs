use crate::mail::datatypes::LabelColor;
use proton_mail_common::actions::{
    ConversationActionKind as RealConversationActionKind,
    ConversationAvailableAction as RealConversationAvailabaleAction,
};
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ConversationAvailableAction {
    /// Enum based action describer
    pub action: ConversationActionKind,
    /// Conversation::local_id field
    pub local_id: u64,
    /// Identificator for FE
    pub static_id: String,
}

impl From<RealConversationAvailabaleAction> for ConversationAvailableAction {
    fn from(value: RealConversationAvailabaleAction) -> Self {
        ConversationAvailableAction {
            action: value.action.into(),
            local_id: value.local_id.into(),
            static_id: value.static_id.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ConversationActionKind {
    Move {
        label_id: u64,
        name: String,
        color: LabelColor,
    },
    Label {
        label_id: u64,
        name: String,
        color: LabelColor,
    },
    Unlabel {
        label_id: u64,
        name: String,
        color: LabelColor,
    },
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
    Delete,
}

impl From<RealConversationActionKind> for ConversationActionKind {
    fn from(value: RealConversationActionKind) -> Self {
        match value {
            RealConversationActionKind::Delete => ConversationActionKind::Delete,
            RealConversationActionKind::MarkRead => ConversationActionKind::MarkRead,
            RealConversationActionKind::MarkUnread => ConversationActionKind::MarkUnread,
            RealConversationActionKind::Star => ConversationActionKind::Star,
            RealConversationActionKind::Unstar => ConversationActionKind::Unstar,
            RealConversationActionKind::Move { label } => ConversationActionKind::Move {
                label_id: label.label_id.into(),
                name: label.name,
                color: label.color.into(),
            },
            RealConversationActionKind::Label { label } => ConversationActionKind::Label {
                label_id: label.label_id.into(),
                name: label.name,
                color: label.color.into(),
            },
            RealConversationActionKind::Unlabel { label } => ConversationActionKind::Unlabel {
                label_id: label.label_id.into(),
                name: label.name,
                color: label.color.into(),
            },
        }
    }
}
