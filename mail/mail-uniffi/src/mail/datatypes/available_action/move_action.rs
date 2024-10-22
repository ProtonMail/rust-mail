use proton_mail_common::actions::MoveAction as RealMoveAction;
use proton_mail_common::actions::{
    CustomFolderAction as RealCustomFolderAction,
    MovableSystemFolderAction as RealMovableSystemFolderAction,
};

use super::IsSelected;
use crate::mail::datatypes::system_folder::MovableSystemFolder;
use crate::mail::datatypes::{Id, LabelColor};
use crate::{UniffiEnum, UniffiRecord};

/// This enum represents the action of moving a message or conversation to a folder.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum MoveAction {
    /// Move to a system folder (e.g. Inbox, Sent, Archive, Trash).
    SystemFolder(MovableSystemFolderAction),

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
pub struct MovableSystemFolderAction {
    pub local_id: Id,
    pub name: MovableSystemFolder,
    pub is_selected: IsSelected,
}

impl From<RealMovableSystemFolderAction> for MovableSystemFolderAction {
    fn from(value: RealMovableSystemFolderAction) -> Self {
        Self {
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
