use super::{GeneralActions, ReplyAction, SystemFolderAction};
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct ConversationAvailableActions {
    #[builder(default = ReplyAction::all())]
    pub reply_actions: Vec<ReplyAction>, // TODO: check reply_all field
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<SystemFolderAction>,
    #[builder(default = GeneralActions::all())]
    pub general_actions: Vec<GeneralActions>,
}

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
