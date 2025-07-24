use crate::mail::datatypes::MovableSystemFolderAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::{
    AllBottomBarMessageActions as RealAllBottomBarMessageActions,
    BottomBarActions as RealBottomBarActions,
};

/// All actions on messages selection.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct AllBottomBarMessageActions {
    /// Actions hidden in bottom bar, but to be shown in corresponding More action
    pub hidden_bottom_bar_actions: Vec<BottomBarActions>,

    /// Actions that must be in the bottom bar
    pub visible_bottom_bar_actions: Vec<BottomBarActions>,
}

impl From<RealAllBottomBarMessageActions> for AllBottomBarMessageActions {
    fn from(value: RealAllBottomBarMessageActions) -> Self {
        Self {
            hidden_bottom_bar_actions: value.hidden_bottom_bar_actions.map_vec(),
            visible_bottom_bar_actions: value.visible_bottom_bar_actions.map_vec(),
        }
    }
}

/// Enumeration grouping all possible actions for BottomBar
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum BottomBarActions {
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

impl From<RealBottomBarActions> for BottomBarActions {
    fn from(value: RealBottomBarActions) -> Self {
        match value {
            RealBottomBarActions::LabelAs => Self::LabelAs,
            RealBottomBarActions::MarkRead => Self::MarkRead,
            RealBottomBarActions::MarkUnread => Self::MarkUnread,
            RealBottomBarActions::More => Self::More,
            RealBottomBarActions::MoveTo => Self::MoveTo,
            RealBottomBarActions::MoveToSystemFolder(label) => {
                Self::MoveToSystemFolder(label.into())
            }
            RealBottomBarActions::NotSpam(label) => Self::NotSpam(label.into()),
            RealBottomBarActions::PermanentDelete => Self::PermanentDelete,
            RealBottomBarActions::Star => Self::Star,
            RealBottomBarActions::Unstar => Self::Unstar,
            RealBottomBarActions::Snooze => Self::Snooze,
        }
    }
}
