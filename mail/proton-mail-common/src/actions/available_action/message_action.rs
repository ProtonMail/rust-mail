use super::{GeneralActions, ReplyAction, SystemFolderAction};
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct MessageAvailableActions {
    #[builder(default = ReplyAction::all())]
    pub reply_actions: Vec<ReplyAction>, // TODO: check reply_all field
    pub message_actions: Vec<MessageAction>,
    pub move_actions: Vec<SystemFolderAction>,
    #[builder(default = GeneralActions::all())]
    pub general_actions: Vec<GeneralActions>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageAction {
    Star,
    Unstar,
    Pin,
    Unpin,
    LabelAs,
    MarkRead,
    MarkUnread,
    Delete,
}
