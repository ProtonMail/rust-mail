use super::GeneralActions;
use crate::mail::datatypes::MoveItemAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::ConversationAvailableActions as RealConversationAvailableActions;
use proton_mail_common::actions::OldConversationAction as RealConversationAction;

/// Struct to reflect the view what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct ConversationAvailableActions {
    pub conversation_actions: Vec<OldConversationAction>,
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
pub enum OldConversationAction {
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

impl From<RealConversationAction> for OldConversationAction {
    fn from(value: RealConversationAction) -> Self {
        match value {
            RealConversationAction::Star => OldConversationAction::Star,
            RealConversationAction::Unstar => OldConversationAction::Unstar,
            RealConversationAction::Pin => OldConversationAction::Pin,
            RealConversationAction::Unpin => OldConversationAction::Unpin,
            RealConversationAction::LabelAs => OldConversationAction::LabelAs,
            RealConversationAction::MarkRead => OldConversationAction::MarkRead,
            RealConversationAction::MarkUnread => OldConversationAction::MarkUnread,
            RealConversationAction::Delete => OldConversationAction::Delete,
            RealConversationAction::Snooze => OldConversationAction::Snooze,
        }
    }
}
