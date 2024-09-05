use super::{GeneralActions, MoveAction, ReplyAction};
use crate::{UniffiEnum, UniffiRecord};
use itertools::Itertools;
use proton_mail_common::actions::ConversationAction as RealConversationAction;
use proton_mail_common::actions::ConversationAvailableActions as RealConversationAvailableActions;

/// Struct to reflect the view what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct ConversationAvailableActions {
    pub reply_actions: Vec<ReplyAction>,
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<MoveAction>,
    pub general_actions: Vec<GeneralActions>,
}

impl From<RealConversationAvailableActions> for ConversationAvailableActions {
    fn from(value: RealConversationAvailableActions) -> Self {
        ConversationAvailableActions {
            reply_actions: value.reply_actions.into_iter().map_into().collect(),
            conversation_actions: value.conversation_actions.into_iter().map_into().collect(),
            move_actions: value.move_actions.into_iter().map_into().collect(),
            general_actions: value.general_actions.into_iter().map_into().collect(),
        }
    }
}

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
        }
    }
}
