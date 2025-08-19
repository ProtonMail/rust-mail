use crate::mail::datatypes::MovableSystemFolderAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::{
    AllConversationActions as RealAllConversationActions,
    ConversationAction as RealConversationAction,
    ConversationActionSheet as RealConversationActionSheet,
};

/// All actions on conversation selection.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct AllConversationActions {
    /// Actions hidden in conversation toolbar, but to be shown in corresponding More action
    pub hidden_list_actions: Vec<ConversationAction>,

    /// Actions that must be in the conversation toolbar
    pub visible_list_actions: Vec<ConversationAction>,
}

impl From<RealAllConversationActions> for AllConversationActions {
    fn from(value: RealAllConversationActions) -> Self {
        Self {
            hidden_list_actions: value.hidden_list_actions.map_vec(),
            visible_list_actions: value.visible_list_actions.map_vec(),
        }
    }
}

/// Conversation action sheet grouped by categories for UI display.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct ConversationActionSheet {
    /// Core conversation actions (Mark Read/Unread, Star/Unstar, LabelAs)
    pub conversation_actions: Vec<ConversationAction>,

    /// Movement-related actions (Archive, Trash, Move, Snooze, etc.)
    pub move_actions: Vec<ConversationAction>,
}

impl From<RealConversationActionSheet> for ConversationActionSheet {
    fn from(value: RealConversationActionSheet) -> Self {
        Self {
            conversation_actions: value.conversation_actions.map_vec(),
            move_actions: value.move_actions.map_vec(),
        }
    }
}

/// Enumeration grouping all possible actions for Conversation Toolbar
/// Note: ConversationAction = ListAction, so this maps all list action variants
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum ConversationAction {
    LabelAs,
    MarkRead,
    MarkUnread,
    More,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),
    NotSpam(MovableSystemFolderAction),
    PermanentDelete,
    Star,
    Unstar,
    Snooze,
}

impl From<RealConversationAction> for ConversationAction {
    fn from(value: RealConversationAction) -> Self {
        match value {
            RealConversationAction::LabelAs => Self::LabelAs,
            RealConversationAction::MarkRead => Self::MarkRead,
            RealConversationAction::MarkUnread => Self::MarkUnread,
            RealConversationAction::More => Self::More,
            RealConversationAction::MoveTo => Self::MoveTo,
            RealConversationAction::MoveToSystemFolder(label) => {
                Self::MoveToSystemFolder(label.into())
            }
            RealConversationAction::NotSpam(label) => Self::NotSpam(label.into()),
            RealConversationAction::PermanentDelete => Self::PermanentDelete,
            RealConversationAction::Star => Self::Star,
            RealConversationAction::Unstar => Self::Unstar,
            RealConversationAction::Snooze => Self::Snooze,
        }
    }
}
