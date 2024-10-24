use super::{GeneralActions, ReplyAction};
use crate::actions::MovableSystemFolderAction;
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct ConversationAvailableActions {
    #[builder(default = ReplyAction::single_address())]
    pub reply_actions: Vec<ReplyAction>, // TODO: check reply_all field
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<MovableSystemFolderAction>,
    #[builder(default = GeneralActions::all())]
    pub general_actions: Vec<GeneralActions>,
}

/// Actions that can be taken on a conversation.
/// It reflects with low granularity what can be done.
/// Each of the options are meant to display a button.
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
