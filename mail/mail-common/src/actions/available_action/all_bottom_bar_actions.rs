#[cfg(test)]
#[path = "../../tests/actions/available_actions/action_bottom_bar.rs"]
mod tests;

use crate::actions::MovableSystemFolderAction;
use crate::datatypes::{MobileActions, SystemLabelId};
use proton_core_api::services::proton::LabelId;
use tracing::warn;

/// All actions available from bottom bar for messages
///
#[derive(Debug, Clone, PartialEq)]
pub struct AllBottomBarMessageActions {
    pub hidden_bottom_bar_actions: Vec<BottomBarActions>,
    pub visible_bottom_bar_actions: Vec<BottomBarActions>,
}

/// Actions available from bottom bar for messages
///
#[derive(Clone, Eq, Hash, PartialEq, derive_more::derive::Debug)]
pub enum BottomBarActions {
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
}

impl BottomBarActions {
    /// Convert a MobileAction item into a BottomBarActions
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_mobile_actions(
        mobile_actions: &MobileActions,
        any_unread: bool,
        all_starred: bool,
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Option<Self> {
        match mobile_actions {
            MobileActions::Archive => Some(Self::toggle_archive(current_label, inbox, archive)),
            MobileActions::Label => Some(Self::LabelAs),
            MobileActions::Move => Some(Self::MoveTo),
            MobileActions::Spam => Some(Self::toggle_spam(current_label, inbox, spam)),
            MobileActions::ToggleRead => Some(Self::toggle_read(any_unread)),
            MobileActions::ToggleStar => Some(Self::toggle_star(all_starred)),
            MobileActions::Trash => Some(Self::toggle_trash(current_label, trash)),
            _ => {
                warn!("Invalid mobile action type: {mobile_actions:?}");
                None
            }
        }
    }

    pub(crate) fn toggle_read(any_unread: bool) -> Self {
        if any_unread {
            BottomBarActions::MarkRead
        } else {
            BottomBarActions::MarkUnread
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

    /// Get actions not displayed in bottom_bar when selecting messages or actions
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hidden_bottom_bar_actions(
        current_label: LabelId,
        any_unread: bool,
        any_read: bool,
        any_unstarred: bool,
        any_starred: bool,
        visible_actions: &[BottomBarActions],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Vec<BottomBarActions> {
        let mut result = Vec::new();

        // Mark as read/unread
        if any_unread && !visible_actions.contains(&BottomBarActions::MarkRead) {
            result.push(BottomBarActions::MarkRead);
        }
        if any_read && !visible_actions.contains(&BottomBarActions::MarkUnread) {
            result.push(BottomBarActions::MarkUnread);
        }
        // Star/Unstar
        if any_unstarred && !visible_actions.contains(&BottomBarActions::Star) {
            result.push(BottomBarActions::Star);
        }
        if any_starred && !visible_actions.contains(&BottomBarActions::Unstar) {
            result.push(BottomBarActions::Unstar);
        }
        // Move to...
        if !visible_actions.contains(&BottomBarActions::MoveTo) {
            result.push(BottomBarActions::MoveTo);
        }
        // Label as...
        if !visible_actions.contains(&BottomBarActions::LabelAs) {
            result.push(BottomBarActions::LabelAs);
        }
        // Move to Inbox
        if [LabelId::trash(), LabelId::archive()].contains(&current_label)
            && !visible_actions.contains(&BottomBarActions::MoveToSystemFolder(inbox.clone()))
        {
            result.push(BottomBarActions::MoveToSystemFolder(inbox.clone()));
        }
        if current_label == LabelId::spam()
            && !visible_actions.contains(&BottomBarActions::NotSpam(inbox.clone()))
        {
            result.push(BottomBarActions::NotSpam(inbox.clone()));
        }
        // Archive
        if current_label != LabelId::archive()
            && !visible_actions.contains(&BottomBarActions::MoveToSystemFolder(archive.clone()))
        {
            result.push(BottomBarActions::MoveToSystemFolder(archive.clone()));
        }
        // Move to Spam
        if ![LabelId::trash(), LabelId::spam()].contains(&current_label)
            && !visible_actions.contains(&BottomBarActions::MoveToSystemFolder(spam.clone()))
        {
            result.push(BottomBarActions::MoveToSystemFolder(spam.clone()));
        }
        // Move to Trash
        if ![LabelId::trash(), LabelId::spam()].contains(&current_label)
            && !visible_actions.contains(&BottomBarActions::MoveToSystemFolder(trash.clone()))
        {
            result.push(BottomBarActions::MoveToSystemFolder(trash.clone()));
        }
        result
    }
}
