use super::GeneralActions;
use crate::actions::RealMoveItemAction;
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct ConversationAvailableActions {
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<RealMoveItemAction>,
    #[builder(default = GeneralActions::all())]
    pub general_actions: Vec<GeneralActions>,
}

/// Actions that can be taken on a conversation.
/// It reflects with low granularity what can be done.
/// Each of the options is meant to display a button.
///
#[derive(Debug, Clone, PartialEq)]
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
