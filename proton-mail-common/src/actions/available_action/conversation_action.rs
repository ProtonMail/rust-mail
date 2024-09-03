use super::{GeneralActions, MoveAction, ReplyAction};
use typed_builder::TypedBuilder;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, TypedBuilder)]
pub struct ConversationAvailableActions {
    /// Conversation::local_id field
    // pub local_ids: Vec<LocalId>,

    #[builder(default = ReplyAction::all())]
    pub reply_actions: Vec<ReplyAction>,
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<MoveAction>,
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

// impl ConversationAction {
//     pub fn vec<'a>(iter: impl IntoIterator<Item = LabelAction>) -> Vec<Self> {
//         iter.into_iter()
//             .map(|label| ConversationAction::Label(label))
//             .collect()
//     }
// }
