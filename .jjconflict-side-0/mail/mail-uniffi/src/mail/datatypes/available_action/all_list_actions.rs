use crate::mail::datatypes::MovableSystemFolderAction;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::actions::{
    AllListActions as RealAllListActions, ListAction as RealListActions,
};

/// All actions on messages selection.
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct AllListActions {
    /// Actions hidden in list toolbar, but to be shown in corresponding More action
    pub hidden_list_actions: Vec<ListActions>,

    /// Actions that must be in the list toolbar
    pub visible_list_actions: Vec<ListActions>,
}

impl From<RealAllListActions> for AllListActions {
    fn from(value: RealAllListActions) -> Self {
        Self {
            hidden_list_actions: value.hidden_list_actions.map_vec(),
            visible_list_actions: value.visible_list_actions.map_vec(),
        }
    }
}

/// Enumeration grouping all possible actions for List Toolbar
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum ListActions {
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

impl From<RealListActions> for ListActions {
    fn from(value: RealListActions) -> Self {
        match value {
            RealListActions::LabelAs => Self::LabelAs,
            RealListActions::MarkRead => Self::MarkRead,
            RealListActions::MarkUnread => Self::MarkUnread,
            RealListActions::More => Self::More,
            RealListActions::MoveTo => Self::MoveTo,
            RealListActions::MoveToSystemFolder(label) => Self::MoveToSystemFolder(label.into()),
            RealListActions::NotSpam(label) => Self::NotSpam(label.into()),
            RealListActions::PermanentDelete => Self::PermanentDelete,
            RealListActions::Star => Self::Star,
            RealListActions::Unstar => Self::Unstar,
            RealListActions::Snooze => Self::Snooze,
        }
    }
}
