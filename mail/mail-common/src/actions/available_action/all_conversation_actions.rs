use super::GeneralActions;
use crate::actions::{AllListActions, MoveItemAction};
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct ConversationAvailableActions {
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<MoveItemAction>,
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
    Snooze,
}

/// As for this moment the conversation actions are exactly the same as the list actions.
///
/// I introduce type alias as a means if that ever changes in the future.
pub type AllConversationActions = AllListActions;
