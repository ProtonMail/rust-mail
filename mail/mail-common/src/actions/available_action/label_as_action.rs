#[cfg(test)]
#[path = "../../tests/actions/available_actions/label_as_action.rs"]
mod tests;

use proton_core_common::{
    datatypes::{LabelColor, LabelType, LocalLabelId},
    models::Label,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// This struct represents a label that can be used as an action.
///
pub struct LabelAsAction {
    pub label_id: LocalLabelId,

    pub name: String,

    pub color: LabelColor,

    pub order: u32,

    /// This field is used to determine if the label is selected or not
    /// for given list of messages or conversations.
    ///
    /// Option<bool> is used to represent three states:
    /// * Some(true) - All of the label occurences across all of messages/conversations have them assigned.
    /// * Some(false) - None of the label occurences across all of messages/conversations have them assigned.
    /// * None - Some of the label occurences across all messages/conversations have them assigned and some don't.
    ///
    /// Option type was chosen over dedicated enum to make it easier to calculate the final state of the label.
    /// Due to the fact algorithm calculate this value multiple times and then modify already existing fields.
    pub is_selected: Option<bool>,
}

impl LabelAsAction {
    /// Create a vector of `LabelAsAction` from a vector of `Label`.
    /// It is meant to be called for each item for which action is calculated.
    /// After which all those vectors joined together should be passed to `finalize` method.
    /// In order to properly calculate the `is_selected` field.
    ///
    /// # Arguments
    ///
    /// * `iter` - An iterator over the labels. Expected to be sorted by `display_order`.
    /// * `is_selected` - A function that determines if the label is selected for the given item.
    ///
    pub fn vec<'a>(
        iter: impl IntoIterator<Item = &'a Label>,
        is_selected: impl Fn(&Label) -> bool,
    ) -> Vec<Self> {
        iter.into_iter()
            .filter_map(|label| match label.label_type {
                LabelType::Label => Some(LabelAsAction {
                    label_id: label.local_id?,
                    name: label.name.clone(),
                    color: label.color.clone(),
                    order: label.display_order,
                    is_selected: Some(is_selected(label)),
                }),
                _ => None,
            })
            .collect()
    }

    /// Method ustilzes map to calculate the final state of the label.
    /// It requires all of the duplicated labels to be present from the `vec` method.
    ///
    pub fn finalize(actions: impl IntoIterator<Item = LabelAsAction>) -> Vec<Self> {
        let mut map = LabelAsActionMap::new();

        for action in actions {
            map.insert(action.label_id, action);
        }

        map.drain()
    }
}

struct LabelAsActionMap {
    map: BTreeMap<LocalLabelId, Vec<LabelAsAction>>,
}

impl LabelAsActionMap {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    fn insert(&mut self, label_id: LocalLabelId, action: LabelAsAction) {
        self.map.entry(label_id).or_default().push(action);
    }

    fn drain(self) -> Vec<LabelAsAction> {
        self.map
            .into_iter()
            .filter_map(|(_, mut actions)| {
                if actions.is_empty() {
                    return None;
                }

                let all_are_selected = actions.iter().all(|x| x.is_selected.unwrap_or(false));

                if all_are_selected {
                    actions.pop()
                } else {
                    let is_partially_selected =
                        actions.iter().any(|x| x.is_selected.unwrap_or(false));
                    let mut action = actions.pop()?;

                    if is_partially_selected {
                        action.is_selected = None;
                    } else {
                        action.is_selected = Some(false);
                    }

                    Some(action)
                }
            })
            .collect()
    }
}
