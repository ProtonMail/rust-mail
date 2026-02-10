use crate::mail::datatypes::MovableSystemFolderAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::{
    AllMessageActions as RealAllMessageActions, MessageAction as RealMessageAction,
    MessageActionSheet as RealMessageActionSheet,
};

/// All actions on message selection.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct AllMessageActions {
    /// Actions hidden in message toolbar, but to be shown in corresponding More action
    pub hidden_message_actions: Vec<MessageAction>,

    /// Actions that must be in the message toolbar
    pub visible_message_actions: Vec<MessageAction>,
}

impl From<RealAllMessageActions> for AllMessageActions {
    fn from(value: RealAllMessageActions) -> Self {
        Self {
            hidden_message_actions: value.hidden_message_actions.map_vec(),
            visible_message_actions: value.visible_message_actions.map_vec(),
        }
    }
}

/// Message action sheet grouped by categories for UI display.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct MessageActionSheet {
    /// Actions for replying (Reply, ReplyAll, Forward)
    pub reply_actions: Vec<MessageAction>,

    /// Core message actions (Mark Read/Unread, Star/Unstar, etc.)
    pub message_actions: Vec<MessageAction>,

    /// Movement-related actions (Archive, Trash, Move, etc.)
    pub move_actions: Vec<MessageAction>,

    /// General utility actions (Print, Save PDF, View Headers, etc.)
    pub general_actions: Vec<MessageAction>,
}

impl From<RealMessageActionSheet> for MessageActionSheet {
    fn from(value: RealMessageActionSheet) -> Self {
        Self {
            reply_actions: value.reply_actions.map_vec(),
            message_actions: value.message_actions.map_vec(),
            move_actions: value.move_actions.map_vec(),
            general_actions: value.general_actions.map_vec(),
        }
    }
}

/// Enumeration grouping all possible actions for Message Toolbar
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum MessageAction {
    // Read state
    MarkRead,
    MarkUnread,

    // Star state
    Star,
    Unstar,

    // Organization
    LabelAs,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),
    NotSpam(MovableSystemFolderAction),
    PermanentDelete,

    // Communication
    Reply,
    ReplyAll,
    Forward,

    // Export/View
    Print,
    ViewHeaders,
    ViewHTML,
    ViewInLightMode,
    ViewInDarkMode,

    // Utility
    ReportPhishing,
    More,
}

impl From<RealMessageAction> for MessageAction {
    fn from(value: RealMessageAction) -> Self {
        match value {
            RealMessageAction::MarkRead => Self::MarkRead,
            RealMessageAction::MarkUnread => Self::MarkUnread,
            RealMessageAction::Star => Self::Star,
            RealMessageAction::Unstar => Self::Unstar,
            RealMessageAction::LabelAs => Self::LabelAs,
            RealMessageAction::MoveTo => Self::MoveTo,
            RealMessageAction::MoveToSystemFolder(label) => Self::MoveToSystemFolder(label.into()),
            RealMessageAction::NotSpam(label) => Self::NotSpam(label.into()),
            RealMessageAction::Reply => Self::Reply,
            RealMessageAction::ReplyAll => Self::ReplyAll,
            RealMessageAction::Forward => Self::Forward,
            RealMessageAction::Print => Self::Print,
            RealMessageAction::ViewHeaders => Self::ViewHeaders,
            RealMessageAction::ViewHTML => Self::ViewHTML,
            RealMessageAction::ViewInLightMode => Self::ViewInLightMode,
            RealMessageAction::ViewInDarkMode => Self::ViewInDarkMode,
            RealMessageAction::PermanentDelete => Self::PermanentDelete,
            RealMessageAction::ReportPhishing => Self::ReportPhishing,
            RealMessageAction::More => Self::More,
        }
    }
}
