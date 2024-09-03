#[cfg(test)]
#[path = "../../tests/actions/available_actions/label_as_action.rs"]
mod tests;

use crate::{
    datatypes::{LabelColor, LabelType},
    models::Label,
};
use proton_core_common::datatypes::LocalId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelAsAction {
    pub label_id: LocalId,
    pub name: String,
    pub color: LabelColor,
    pub is_selected: Option<bool>,
}

impl LabelAsAction {
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
                    is_selected: Some(is_selected(label)),
                }),
                _ => None,
            })
            .collect()
    }

    pub fn finalize(actions: impl IntoIterator<Item = LabelAsAction>) -> Vec<Self> {
        let mut map = LabelAsActionMap::new();

        for action in actions {
            map.insert(action.label_id, action);
        }

        map.drain()
    }
}

struct LabelAsActionMap {
    map: BTreeMap<LocalId, Vec<LabelAsAction>>,
}

impl LabelAsActionMap {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    fn insert(&mut self, label_id: LocalId, action: LabelAsAction) {
        self.map.entry(label_id).or_default().push(action);
    }

    fn drain(self) -> Vec<LabelAsAction> {
        self.map
            .into_iter()
            .filter_map(|(_, mut actions)| {
                if actions.is_empty() {
                    return None;
                }

                let is_selected = actions.iter().all(|x| x.is_selected.unwrap_or(false));

                if is_selected {
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
