use proton_core_common::datatypes::LocalId;

use crate::{
    datatypes::{LabelColor, LabelType, SystemLabel},
    models::Label,
};

#[derive(Debug, Clone, PartialEq)]
pub enum MoveAction {
    SystemFolder(SystemFolderAction),
    CustomFolder(CustomFolderAction),
}

impl MoveAction {
    pub fn vec<'a>(
        iter: impl IntoIterator<Item = &'a Label>,
        is_selected: impl Fn(&Label) -> bool,
    ) -> Vec<Self> {
        iter.into_iter()
            .filter_map(|label| match label.label_type {
                LabelType::System => Some(MoveAction::SystemFolder(SystemFolderAction {
                    local_id: label.local_id?,
                    name: SystemLabel::new(label)?,
                    is_selected: is_selected(label),
                })),

                LabelType::Folder => Some(MoveAction::CustomFolder(CustomFolderAction {
                    local_id: label.local_id?,
                    name: label.name.clone(),
                    color: label.color.clone(),
                    parent: label.local_parent_id,
                    is_selected: is_selected(label),
                })),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemFolderAction {
    pub local_id: LocalId,
    pub name: SystemLabel,
    pub is_selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomFolderAction {
    pub local_id: LocalId,
    pub name: String,
    pub color: LabelColor,
    pub parent: Option<LocalId>,
    pub is_selected: bool,
}
