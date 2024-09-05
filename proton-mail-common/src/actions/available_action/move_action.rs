#[cfg(test)]
#[path = "../../tests/actions/available_actions/move_action.rs"]
mod tests;

use crate::{
    datatypes::{LabelColor, LabelType, SystemLabel},
    models::Label,
};
use proton_core_common::datatypes::LocalId;
use std::collections::BTreeMap;

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
                    name: SystemLabel::new(label).filter(|sl| sl.is_movable_folder())?,
                    is_selected: Some(is_selected(label)),
                })),

                LabelType::Folder => Some(MoveAction::CustomFolder(CustomFolderAction {
                    local_id: label.local_id?,
                    name: label.name.clone(),
                    color: label.color.clone(),
                    parent: label.local_parent_id,
                    is_selected: Some(is_selected(label)),
                })),
                _ => None,
            })
            .collect()
    }

    pub fn finalize(actions: impl IntoIterator<Item = MoveAction>) -> Vec<Self> {
        let mut map = MoveActionMap::new();

        for action in actions {
            match &action {
                MoveAction::SystemFolder(system_action) => {
                    map.insert(system_action.local_id, action);
                }
                MoveAction::CustomFolder(system_action) => {
                    map.insert(system_action.local_id, action);
                }
            }
        }

        map.drain()
    }

    pub fn system(actions: impl IntoIterator<Item = MoveAction>) -> Vec<SystemFolderAction> {
        actions
            .into_iter()
            .filter_map(|action| match action {
                MoveAction::SystemFolder(action) => Some(action),
                _ => None,
            })
            .collect()
    }

    fn is_selected(&self) -> Option<bool> {
        match self {
            MoveAction::SystemFolder(action) => action.is_selected,
            MoveAction::CustomFolder(action) => action.is_selected,
        }
    }

    fn set_selected(&mut self, selected: Option<bool>) {
        match self {
            MoveAction::SystemFolder(action) => action.is_selected = selected,
            MoveAction::CustomFolder(action) => action.is_selected = selected,
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub fn set_local_id(&mut self, local_id: LocalId) {
        match self {
            MoveAction::SystemFolder(action) => action.local_id = local_id,
            MoveAction::CustomFolder(action) => action.local_id = local_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemFolderAction {
    pub local_id: LocalId,
    pub name: SystemLabel,

    /// This field is used to determine if the folder is selected or not
    /// for given list of messages or conversations.
    ///
    /// Option<bool> is used to represent three states:
    /// * Some(true) - All of the folder occurences across all of messages/conversations have them assigned.
    /// * Some(false) - None of the folder occurences across all of messages/conversations have them assigned.
    /// * None - Some of the folder occurences across all messages/conversations have them assigned and some don't.
    ///
    /// Option type was chosen over dedicated enum to make it easier to calculate the final state of the folder.
    /// Due to the fact algorithm calculate this value multiple times and then modify already existing fields.
    pub is_selected: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomFolderAction {
    pub local_id: LocalId,
    pub name: String,
    pub color: LabelColor,
    pub parent: Option<LocalId>,

    /// This field is used to determine if the folder is selected or not
    /// for given list of messages or conversations.
    ///
    /// For more information check the documentation of analaogical field in [SystemFolderAction].
    pub is_selected: Option<bool>,
}

struct MoveActionMap {
    map: BTreeMap<LocalId, Vec<MoveAction>>,
}

impl MoveActionMap {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    fn insert(&mut self, label_id: LocalId, action: MoveAction) {
        self.map.entry(label_id).or_default().push(action);
    }

    fn drain(self) -> Vec<MoveAction> {
        self.map
            .into_iter()
            .filter_map(|(_, mut actions)| {
                if actions.is_empty() {
                    return None;
                }

                let is_selected = actions.iter().all(|x| x.is_selected().unwrap_or(false));

                if is_selected {
                    actions.pop()
                } else {
                    let is_partially_selected =
                        actions.iter().any(|x| x.is_selected().unwrap_or(false));
                    let mut action = actions.pop()?;

                    if is_partially_selected {
                        action.set_selected(None);
                    } else {
                        action.set_selected(Some(false))
                    }

                    Some(action)
                }
            })
            .collect()
    }
}
