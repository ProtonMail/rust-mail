use super::LabelAction;
use proton_core_common::datatypes::LocalId;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq)]
pub struct MessageAvailableAction {
    /// Enum based action describer
    pub action: MessageActionKind,

    /// Message::local_id field
    pub local_id: LocalId,

    /// Identificator for FE
    pub static_id: &'static str,
}

impl MessageAvailableAction {
    /// Creates a new instance of MessageAvailableAction
    /// and automates assignment of static_id
    ///
    pub fn new(action: MessageActionKind, label_id: LocalId) -> Self {
        let static_id = action.static_id();

        Self {
            action,
            local_id: label_id,
            static_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageActionKind {
    Move { label: LabelAction },
    Label { label: LabelAction },
    Unlabel { label: LabelAction },
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
    Delete,
}

impl MessageActionKind {
    pub fn static_id(&self) -> &'static str {
        match self {
            Self::Move { .. } => "move",
            Self::Label { .. } => "label",
            Self::Unlabel { .. } => "unlabel",
            Self::MarkRead => "mark_read",
            Self::MarkUnread => "mark_unread",
            Self::Star => "star",
            Self::Unstar => "unstar",
            Self::Delete => "delete",
        }
    }
}
