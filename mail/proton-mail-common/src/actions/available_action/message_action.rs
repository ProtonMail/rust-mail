use super::{GeneralActions, MoveAction, ReplyAction};
use proton_core_common::datatypes::LocalId;
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct MessageAvailableActions {
    /// Message::local_id field
    // pub local_ids: Vec<LocalId>,

    #[builder(default = ReplyAction::all())]
    pub reply_actions: Vec<ReplyAction>,
    pub message_actions: Vec<MessageAction>,
    pub move_actions: Vec<MoveAction>,
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

// impl MessageAction {
//     pub fn validate(actions: &[MessageAction]) -> bool {
//         for one in actions {
//             for other in actions {
//                 match (one, other) {
//                     (MessageAction::Star, MessageAction::Unstar)
//                     | (MessageAction::Pin, MessageAction::Unpin)
//                     | (MessageAction::MarkRead, MessageAction::MarkUnread) => return false,
//                     _ => (),
//                 }
//             }
//         }

//         true
//     }
// }

// macro_rules! message_actions {
//     ($($action: tt),*) => {{
//         vec![$(MessageAction::$action),*]
//     }};
// }

// impl MessageAction {
//     pub fn vec<'a>(iter: impl IntoIterator<Item = LabelAction>) -> Vec<Self> {
//         iter.into_iter()
//             .map(|label| MessageAction::Label(label))
//             .collect()
//     }
// }
