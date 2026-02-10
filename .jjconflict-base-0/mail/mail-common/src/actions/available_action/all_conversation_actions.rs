use crate::actions::{AllListActions, ListAction};

/// As for this moment the conversation actions are exactly the same as the list actions.
///
/// I introduce type alias as a means if that ever changes in the future.
pub type AllConversationActions = AllListActions;
pub type ConversationAction = ListAction;

/// Struct to reflect what kind of actions
/// could be taken upon the conversation.
///
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConversationActionSheet {
    pub conversation_actions: Vec<ConversationAction>,
    pub move_actions: Vec<ConversationAction>,
}

impl From<AllConversationActions> for ConversationActionSheet {
    fn from(value: AllConversationActions) -> Self {
        let mut this = Self::default();
        let all_actions = [value.visible_list_actions, value.hidden_list_actions].concat();

        all_actions.iter().for_each(|action| {
            if action.is_move_action() {
                this.move_actions.push(*action);
            } else if action.is_conversation_action() {
                this.conversation_actions.push(*action);
            }
        });

        this
    }
}

impl ConversationAction {
    pub fn is_move_action(&self) -> bool {
        matches!(
            self,
            ConversationAction::MoveTo
                | ConversationAction::MoveToSystemFolder(_)
                | ConversationAction::NotSpam(_)
                | ConversationAction::PermanentDelete
                | ConversationAction::Snooze
        )
    }

    pub fn is_conversation_action(&self) -> bool {
        matches!(
            self,
            ConversationAction::Star
                | ConversationAction::Unstar
                | ConversationAction::LabelAs
                | ConversationAction::MarkRead
                | ConversationAction::MarkUnread
        )
    }
}
