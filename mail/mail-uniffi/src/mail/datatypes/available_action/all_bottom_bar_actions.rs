use crate::mail::datatypes::SystemLabel;
use crate::{UniffiEnum, UniffiRecord};
use itertools::Itertools;
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
            hidden_bottom_bar_actions: value
                .hidden_bottom_bar_actions
                .into_iter()
                .map_into()
                .collect(),
            visible_bottom_bar_actions: value
                .visible_bottom_bar_actions
                .into_iter()
                .map_into()
                .collect(),
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
    MoveToSystemFolder(SystemLabel),
    NotSpam,
    PermanentDelete,
    Star,
    Unstar,
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
            RealBottomBarActions::NotSpam => Self::NotSpam,
            RealBottomBarActions::PermanentDelete => Self::PermanentDelete,
            RealBottomBarActions::Star => Self::Star,
            RealBottomBarActions::Unstar => Self::Unstar,
        }
    }
}
