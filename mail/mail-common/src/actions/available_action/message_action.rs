use super::{GeneralActions, ReplyAction};
use crate::actions::MoveItemAction;
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct MessageAvailableActions {
    #[builder(default = ReplyAction::single_address())]
    pub reply_actions: Vec<ReplyAction>, // TODO: check reply_all field
    pub message_actions: Vec<MessageAction>,
    pub move_actions: Vec<MoveItemAction>,
    pub general_actions: Vec<GeneralActions>,
}

/// Actions that can be taken on a message.
/// It reflects with low granularity what can be done.
/// Each of the options is meant to display a button.
///
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
