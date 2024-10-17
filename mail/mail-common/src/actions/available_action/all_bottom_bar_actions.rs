#[cfg(test)]
#[path = "../../tests/actions/available_actions/action_bottom_bar.rs"]
mod tests;

use crate::datatypes::system_label::SystemLabel;
use crate::datatypes::{MobileActions, SystemLabelId};
use proton_core_common::datatypes::LabelId;
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
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum BottomBarActions {
    LabelAs,
    MarkRead,
    MarkUnread,
    More,
    MoveTo,
    MoveToSystemFolder(SystemLabel),
    NotSpam,
    PermanentDelete,
    Star,
    Unstar,
}

impl BottomBarActions {
    pub(crate) fn from_mobile_actions(
        mobile_actions: &MobileActions,
        any_unread: bool,
        all_starred: bool,
        current_label: &LabelId,
    ) -> Option<Self> {
        match mobile_actions {
            MobileActions::Archive => Some(Self::toggle_archive(current_label)),
            MobileActions::Label => Some(Self::LabelAs),
            MobileActions::Move => Some(Self::MoveTo),
            MobileActions::Snooze => Some(Self::MoveToSystemFolder(SystemLabel::Snoozed)),
            MobileActions::Spam => Some(Self::toggle_spam(current_label)),
            MobileActions::ToggleRead => Some(Self::toggle_read(any_unread)),
            MobileActions::ToggleStar => Some(Self::toggle_star(all_starred)),
            MobileActions::Trash => Some(Self::toggle_trash(current_label)),
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

    pub(crate) fn toggle_archive(current_label: &LabelId) -> Self {
        if current_label == &LabelId::archive() {
            Self::MoveToSystemFolder(SystemLabel::Inbox)
        } else {
            Self::MoveToSystemFolder(SystemLabel::Archive)
        }
    }

    pub(crate) fn toggle_spam(current_label: &LabelId) -> Self {
        if current_label == &LabelId::spam() {
            Self::NotSpam
        } else if current_label == &LabelId::trash() {
            Self::MoveToSystemFolder(SystemLabel::Inbox)
        } else {
            Self::MoveToSystemFolder(SystemLabel::Spam)
        }
    }

    pub(crate) fn toggle_star(all_starred: bool) -> Self {
        if all_starred {
            Self::Unstar
        } else {
            Self::Star
        }
    }

    pub(crate) fn toggle_trash(current_label: &LabelId) -> Self {
        if [LabelId::trash(), LabelId::spam()].contains(current_label) {
            Self::PermanentDelete
        } else {
            Self::MoveToSystemFolder(SystemLabel::Trash)
        }
    }
}
