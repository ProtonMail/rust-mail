use proton_mail_common::actions::MoveAction as RealMoveAction;
use proton_mail_common::actions::{
    CustomFolderAction as RealCustomFolderAction, SystemFolderAction as RealSystemFolderAction,
};

use crate::mail::datatypes::{Id, LabelColor, SystemLabel};
use crate::{UniffiEnum, UniffiRecord};

use super::IsSelected;

#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum MoveAction {
    SystemFolder(SystemFolderAction),
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

#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct CustomFolderAction {
    pub local_id: Id,
    pub name: String,
    pub color: LabelColor,
    pub parent: Option<Id>, // TODO: This should be a reference to a custom folder
    pub is_selected: IsSelected,
}

impl From<RealCustomFolderAction> for CustomFolderAction {
    fn from(value: RealCustomFolderAction) -> Self {
        CustomFolderAction {
            local_id: value.local_id.into(),
            name: value.name.clone(),
            color: value.color.into(),
            parent: value.parent.map(Into::into),
            is_selected: IsSelected::new(value.is_selected),
        }
    }
}
