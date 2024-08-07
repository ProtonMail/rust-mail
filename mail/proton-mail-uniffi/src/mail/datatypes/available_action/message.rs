use crate::mail::datatypes::LabelColor;
use proton_mail_common::actions::{
    MessageActionKind as RealMessageActionKind,
    MessageAvailableAction as RealMessageAvailabaleAction,
};
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageAvailableAction {
    /// Enum based action describer
    pub action: MessageActionKind,
    /// Message::local_id field
    pub local_id: u64,
    /// Identificator for FE
    pub static_id: String,
}

impl From<RealMessageAvailabaleAction> for MessageAvailableAction {
    fn from(value: RealMessageAvailabaleAction) -> Self {
        MessageAvailableAction {
            action: value.action.into(),
            local_id: value.local_id,
            static_id: value.static_id.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum MessageActionKind {
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

impl From<RealMessageActionKind> for MessageActionKind {
    fn from(value: RealMessageActionKind) -> Self {
        match value {
            RealMessageActionKind::Delete => MessageActionKind::Delete,
            RealMessageActionKind::MarkRead => MessageActionKind::MarkRead,
            RealMessageActionKind::MarkUnread => MessageActionKind::MarkUnread,
            RealMessageActionKind::Star => MessageActionKind::Star,
            RealMessageActionKind::Unstar => MessageActionKind::Unstar,
            RealMessageActionKind::Move { label } => MessageActionKind::Move {
                label_id: label.label_id,
                name: label.name,
                color: label.color.into(),
            },
            RealMessageActionKind::Label { label } => MessageActionKind::Label {
                label_id: label.label_id,
                name: label.name,
                color: label.color.into(),
            },
            RealMessageActionKind::Unlabel { label } => MessageActionKind::Unlabel {
                label_id: label.label_id,
                name: label.name,
                color: label.color.into(),
            },
        }
    }
}
