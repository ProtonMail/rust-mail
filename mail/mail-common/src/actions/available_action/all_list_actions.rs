#[cfg(test)]
#[path = "../../tests/actions/available_actions/all_list_actions.rs"]
mod tests;

use crate::actions::MovableSystemFolderAction;
use crate::datatypes::{MobileAction, SystemLabelId};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use tracing::warn;

/// All actions available from list toolbar for messages
///
#[derive(Debug, Clone, PartialEq)]
pub struct AllListActions {
    pub hidden_list_actions: Vec<ListAction>,
    pub visible_list_actions: Vec<ListAction>,
}

/// Actions available from list toolbar for messages
///
#[derive(Clone, Eq, Hash, PartialEq, derive_more::derive::Debug)]
pub enum ListAction {
    LabelAs,
    MarkRead,
    MarkUnread,
    More,
    MoveTo,
    #[debug("Move to {:?}", _0.name)]
    MoveToSystemFolder(MovableSystemFolderAction),
    #[debug("NotSpam: Move to {:?}", _0.name)]
    NotSpam(MovableSystemFolderAction),
    PermanentDelete,
    Star,
    Unstar,
    Snooze,
}

impl ListAction {
    /// Convert a MobileAction item into a ListActions
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_mobile_actions(
        mobile_actions: &MobileAction,
        any_unread: bool,
        all_starred: bool,
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Option<Self> {
        match mobile_actions {
            MobileAction::Archive => Some(Self::toggle_archive(current_label, inbox, archive)),
            MobileAction::Label => Some(Self::LabelAs),
            MobileAction::Move => Some(Self::MoveTo),
            MobileAction::Spam => Some(Self::toggle_spam(current_label, inbox, spam)),
            MobileAction::ToggleRead => Some(Self::toggle_read(any_unread)),
            MobileAction::ToggleStar => Some(Self::toggle_star(all_starred)),
            MobileAction::Trash => Some(Self::toggle_trash(current_label, trash)),
            MobileAction::Snooze => Self::toggle_snooze(current_label),
            _ => {
                warn!("Invalid mobile action type: {mobile_actions:?}");
                None
            }
        }
    }

    pub(crate) fn toggle_snooze(current_label: &LabelId) -> Option<Self> {
        SystemLabel::from_rid(current_label)
            .filter(|label| label.is_snooze_location())
            .map(|_| Self::Snooze)
    }

    pub(crate) fn toggle_read(any_unread: bool) -> Self {
        if any_unread {
            ListAction::MarkRead
        } else {
            ListAction::MarkUnread
        }
    }

    pub(crate) fn toggle_archive(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
    ) -> Self {
        if current_label == &LabelId::archive() {
            Self::MoveToSystemFolder(inbox.clone())
        } else {
            Self::MoveToSystemFolder(archive.clone())
        }
    }

    pub(crate) fn toggle_spam(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Self {
        if current_label == &LabelId::spam() {
            Self::NotSpam(inbox.clone())
        } else if current_label == &LabelId::trash() {
            Self::MoveToSystemFolder(inbox.clone())
        } else {
            Self::MoveToSystemFolder(spam.clone())
        }
    }

    pub(crate) fn toggle_star(all_starred: bool) -> Self {
        if all_starred {
            Self::Unstar
        } else {
            Self::Star
        }
    }

    pub(crate) fn toggle_trash(current_label: &LabelId, trash: &MovableSystemFolderAction) -> Self {
        if [LabelId::trash(), LabelId::spam()].contains(current_label) {
            Self::PermanentDelete
        } else {
            Self::MoveToSystemFolder(trash.clone())
        }
    }

    /// Get actions not displayed in list toolbar when selecting messages or actions
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hidden_list_actions(
        is_conversation: bool,
        current_label: LabelId,
        any_unread: bool,
        any_read: bool,
        any_unstarred: bool,
        any_starred: bool,
        visible_actions: &[ListAction],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Vec<ListAction> {
        let mut result = Vec::new();

        // Mark as read/unread
        if any_unread && !visible_actions.contains(&ListAction::MarkRead) {
            result.push(ListAction::MarkRead);
        }
        if any_read && !visible_actions.contains(&ListAction::MarkUnread) {
            result.push(ListAction::MarkUnread);
        }
        // Star/Unstar
        if any_unstarred && !visible_actions.contains(&ListAction::Star) {
            result.push(ListAction::Star);
        }
        if any_starred && !visible_actions.contains(&ListAction::Unstar) {
            result.push(ListAction::Unstar);
        }
        // Move to...
        if !visible_actions.contains(&ListAction::MoveTo) {
            result.push(ListAction::MoveTo);
        }
        // Label as...
        if !visible_actions.contains(&ListAction::LabelAs) {
            result.push(ListAction::LabelAs);
        }
        // Snooze
        if is_conversation
            && Self::toggle_snooze(&current_label)
                .filter(|_| !visible_actions.contains(&ListAction::Snooze))
                .is_some()
        {
            result.push(ListAction::Snooze);
        }
        // Move to Inbox
        if [LabelId::trash(), LabelId::archive()].contains(&current_label)
            && !visible_actions.contains(&ListAction::MoveToSystemFolder(inbox.clone()))
        {
            result.push(ListAction::MoveToSystemFolder(inbox.clone()));
        }
        if current_label == LabelId::spam()
            && !visible_actions.contains(&ListAction::NotSpam(inbox.clone()))
        {
            result.push(ListAction::NotSpam(inbox.clone()));
        }
        // Archive
        if current_label != LabelId::archive()
            && !visible_actions.contains(&ListAction::MoveToSystemFolder(archive.clone()))
        {
            result.push(ListAction::MoveToSystemFolder(archive.clone()));
        }
        // Move to Spam
        if ![LabelId::trash(), LabelId::spam()].contains(&current_label)
            && !visible_actions.contains(&ListAction::MoveToSystemFolder(spam.clone()))
        {
            result.push(ListAction::MoveToSystemFolder(spam.clone()));
        }
        // Move to Trash
        if ![LabelId::trash(), LabelId::spam()].contains(&current_label)
            && !visible_actions.contains(&ListAction::MoveToSystemFolder(trash.clone()))
        {
            result.push(ListAction::MoveToSystemFolder(trash.clone()));
        }
        result
    }
}
