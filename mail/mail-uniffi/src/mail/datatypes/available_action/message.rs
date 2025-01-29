use super::{GeneralActions, ReplyAction};
use crate::mail::datatypes::MoveItemAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::MessageAction as RealMessageAction;
use proton_mail_common::actions::MessageAvailableActions as RealMessageAvailableActions;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct MessageAvailableActions {
    pub reply_actions: Vec<ReplyAction>,
    pub message_actions: Vec<MessageAction>,
    pub move_actions: Vec<MoveItemAction>,
    pub general_actions: Vec<GeneralActions>,
}

impl From<RealMessageAvailableActions> for MessageAvailableActions {
    fn from(value: RealMessageAvailableActions) -> Self {
        MessageAvailableActions {
            reply_actions: value.reply_actions.map_vec(),
            message_actions: value.message_actions.map_vec(),
            move_actions: value.move_actions.map_vec(),
            general_actions: value.general_actions.map_vec(),
        }
    }
}

/// Actions that can be taken on a message.
/// It reflects with low granularity what can be done.
/// Each of the options is meant to display a button.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
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

impl From<RealMessageAction> for MessageAction {
    fn from(value: RealMessageAction) -> Self {
        match value {
            RealMessageAction::Star => MessageAction::Star,
            RealMessageAction::Unstar => MessageAction::Unstar,
            RealMessageAction::Pin => MessageAction::Pin,
            RealMessageAction::Unpin => MessageAction::Unpin,
            RealMessageAction::LabelAs => MessageAction::LabelAs,
            RealMessageAction::MarkRead => MessageAction::MarkRead,
            RealMessageAction::MarkUnread => MessageAction::MarkUnread,
            RealMessageAction::Delete => MessageAction::Delete,
        }
    }
}
