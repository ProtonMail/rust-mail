use proton_mail_common::actions::MoveAction as RealMoveAction;
use proton_mail_common::actions::{
    CustomFolderAction as RealCustomFolderAction, SystemFolderAction as RealSystemFolderAction,
};

use crate::mail::datatypes::{Id, LabelColor, SystemLabel};
use crate::{UniffiEnum, UniffiRecord};

use super::IsSelected;

/// This enum represents the action of moving a message or conversation to a folder.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum MoveAction {
    /// Move to a sysem folder (e.g. Inbox, Sent, Archive, Trash).
    SystemFolder(SystemFolderAction),

    /// Move to a custom folder.
    CustomFolder(CustomFolderAction),
}

impl From<RealMoveAction> for MoveAction {
    fn from(value: RealMoveAction) -> Self {
        match value {
            RealMoveAction::SystemFolder(value) => MoveAction::SystemFolder(value.into()),
            RealMoveAction::CustomFolder(value) => MoveAction::CustomFolder(value.into()),
        }
    }
}

/// This struct represents a system folder that can be used as an action.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct SystemFolderAction {
    pub local_id: Id,
    pub name: SystemLabel,
    pub is_selected: IsSelected,
}

impl From<RealSystemFolderAction> for SystemFolderAction {
    fn from(value: RealSystemFolderAction) -> Self {
        SystemFolderAction {
            local_id: value.local_id.into(),
            name: value.name.into(),
            is_selected: IsSelected::new(value.is_selected),
        }
    }
}

/// This struct represents a custom folder that can be used as an action.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct CustomFolderAction {
    pub local_id: Id,

    pub name: String,

    /// Folder color is calculated based on user settings.
    /// None means the folder colors are disabled.
    pub color: Option<LabelColor>,

    /// It holds folder structure as self reference within vector.
    pub children: Vec<CustomFolderAction>,

    pub is_selected: IsSelected,
}

impl From<RealCustomFolderAction> for CustomFolderAction {
    fn from(value: RealCustomFolderAction) -> Self {
        CustomFolderAction {
            local_id: value.local_id.into(),
            name: value.name.clone(),
            color: value.color.map(Into::into),
            children: value.children.into_iter().map(Into::into).collect(),
            is_selected: IsSelected::new(value.is_selected),
        }
    }
}
