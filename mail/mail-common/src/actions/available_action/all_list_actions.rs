#[cfg(test)]
#[path = "../../tests/actions/available_actions/all_list_actions.rs"]
mod tests;

use crate::actions::MovableSystemFolderAction;
use crate::actions::{
    ActionContext, GenericAction, GenericMobileActions, MobileActionsBuilder, SystemFolders,
};
use crate::datatypes::MobileAction;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;

/// All actions available from list toolbar for either conversation groupping enabled and disabled
///
#[derive(Debug, Clone, PartialEq)]
pub struct AllListActions {
    pub hidden_list_actions: Vec<ListAction>,
    pub visible_list_actions: Vec<ListAction>,
}

impl AllListActions {
    #[allow(clippy::too_many_arguments)]
    pub fn from_context(
        is_conversation: bool,
        current_label: LabelId,
        any_unread: bool,
        any_read: bool,
        any_starred: bool,
        all_starred: bool,
        mobile_actions: &[MobileAction],
        inbox: MovableSystemFolderAction,
        archive: MovableSystemFolderAction,
        trash: MovableSystemFolderAction,
        spam: MovableSystemFolderAction,
    ) -> Self {
        let all_read = any_read && !any_unread;

        let context = ActionContext {
            current_label,
            any_unread,
            any_read,
            all_read,
            any_starred,
            all_starred,
            theme: None, // Lists don't need theme-specific actions
            folders: SystemFolders {
                inbox,
                archive,
                trash,
                spam,
            },
            can_reply: false,     // Not applicable for lists
            can_reply_all: false, // Not applicable for lists
            is_conversation,
        };

        let builder = MobileActionsBuilder::<ListAction>::new(context, mobile_actions);
        let (visible_list_actions, hidden_list_actions) = builder.build();

        Self {
            hidden_list_actions,
            visible_list_actions,
        }
    }
}

/// Actions available from list toolbar for messages
///
#[derive(Clone, Copy, Eq, Hash, PartialEq, derive_more::derive::Debug)]
pub enum ListAction {
    LabelAs,
    MarkRead,
    MarkUnread,
    More,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),
    NotSpam(MovableSystemFolderAction),
    PermanentDelete,
    Star,
    Unstar,
    Snooze,
}

impl ListAction {
    pub fn toggle_snooze(current_label: &LabelId) -> Option<Self> {
        SystemLabel::from_rid(current_label)
            .filter(|label| label.is_snooze_location())
            .map(|_| Self::Snooze)
    }
}

// Implementation of conversion from GenericAction to ListAction
impl From<GenericAction> for ListAction {
    fn from(action: GenericAction) -> Self {
        match action {
            GenericAction::MarkRead => Self::MarkRead,
            GenericAction::MarkUnread => Self::MarkUnread,
            GenericAction::Star => Self::Star,
            GenericAction::Unstar => Self::Unstar,
            GenericAction::LabelAs => Self::LabelAs,
            GenericAction::MoveTo => Self::MoveTo,
            GenericAction::MoveToSystemFolder(folder) => Self::MoveToSystemFolder(folder),
            GenericAction::NotSpam(folder) => Self::NotSpam(folder),
            GenericAction::PermanentDelete => Self::PermanentDelete,
            GenericAction::More => Self::More,
        }
    }
}

impl GenericMobileActions for ListAction {
    fn from_mobile_action(
        mobile_action: &crate::datatypes::MobileAction,
        context: &ActionContext,
    ) -> Option<Self> {
        use crate::datatypes::MobileAction::*;
        match mobile_action {
            ToggleRead => Some(Self::toggle_read(context.any_unread)),
            ToggleStar => Some(Self::toggle_star_with_context(
                context.any_starred,
                context.all_starred,
            )),
            Archive => Some(Self::toggle_archive(
                &context.current_label,
                &context.folders.inbox,
                &context.folders.archive,
            )),
            Trash => Some(Self::toggle_trash(
                &context.current_label,
                &context.folders.trash,
            )),
            Spam => Some(Self::toggle_spam(
                &context.current_label,
                &context.folders.inbox,
                &context.folders.spam,
            )),
            Move => Some(Self::MoveTo),
            Label => Some(Self::LabelAs),
            Snooze => Self::toggle_snooze(&context.current_label),
            // Unsupported actions for lists
            Reply | Forward | Print | ViewHeaders | ViewHTML | ToggleLight | ReportPhishing
            | SaveAttachments | SavePDF | SenderEmails | Remind | Other(_) => None,
        }
    }

    fn get_high_priority_actions(context: &ActionContext) -> Vec<Self> {
        if context.is_conversation
            && let Some(snooze) = Self::toggle_snooze(&context.current_label)
        {
            return vec![snooze];
        }
        vec![]
    }

    fn are_counter_actions(action1: &Self, action2: &Self) -> bool {
        use ListAction::*;
        matches!(
            (action1, action2),
            (MarkRead, MarkUnread) | (MarkUnread, MarkRead) | (Star, Unstar) | (Unstar, Star)
        )
    }
}
