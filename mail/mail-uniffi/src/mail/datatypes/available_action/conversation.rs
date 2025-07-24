use super::GeneralActions;
use crate::mail::datatypes::MoveItemAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::ConversationAction as RealConversationAction;
use proton_mail_common::actions::ConversationAvailableActions as RealConversationAvailableActions;

/// Struct to reflect the view what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct ConversationAvailableActions {
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<MoveItemAction>,
    pub general_actions: Vec<GeneralActions>,
}

impl From<RealConversationAvailableActions> for ConversationAvailableActions {
    fn from(value: RealConversationAvailableActions) -> Self {
        ConversationAvailableActions {
            conversation_actions: value.conversation_actions.map_vec(),
            move_actions: value.move_actions.map_vec(),
            general_actions: value.general_actions.map_vec(),
        }
    }
}

/// Actions that can be taken on a conversation.
/// It reflects with low granularity what can be done.
/// Each of the options is meant to display a button.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum ConversationAction {
    Star,
    Unstar,
    Pin,
    Unpin,
    LabelAs,
    MarkRead,
    MarkUnread,
    Delete,
    Snooze,
}

impl From<RealConversationAction> for ConversationAction {
    fn from(value: RealConversationAction) -> Self {
        match value {
            RealConversationAction::Star => ConversationAction::Star,
            RealConversationAction::Unstar => ConversationAction::Unstar,
            RealConversationAction::Pin => ConversationAction::Pin,
            RealConversationAction::Unpin => ConversationAction::Unpin,
            RealConversationAction::LabelAs => ConversationAction::LabelAs,
            RealConversationAction::MarkRead => ConversationAction::MarkRead,
            RealConversationAction::MarkUnread => ConversationAction::MarkUnread,
            RealConversationAction::Delete => ConversationAction::Delete,
            RealConversationAction::Snooze => ConversationAction::Snooze,
        }
    }
}
