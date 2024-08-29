use std::collections::BTreeMap;

use crate::{
    datatypes::{LabelColor, LabelType},
    models::Label,
};
use itertools::Itertools;
use proton_core_common::datatypes::LocalId;

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
        let actions = iter
            .into_iter()
            .filter_map(|label| match label.label_type {
                LabelType::Label => Some(LabelAsAction {
                    label_id: label.local_id?,
                    name: label.name.clone(),
                    color: label.color.clone(),
                    is_selected: Some(is_selected(label)),
                }),
                _ => None,
            })
            .collect_vec();

        let duplicates = actions.clone().into_iter().duplicates().map(|mut item| {
            item.is_selected.take_if(|v| *v);

            item
        });

        duplicates.chain(actions).unique().collect()
    }
}

pub struct LabelAsActionMap {
    map: BTreeMap<LocalId, Vec<LabelAsAction>>,
}

impl LabelAsActionMap {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, label_id: LocalId, action: LabelAsAction) {
        self.map.entry(label_id).or_default().push(action);
    }

    pub fn get(&self, label_id: LocalId) -> Option<&Vec<LabelAsAction>> {
        self.map.get(&label_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&LocalId, &Vec<LabelAsAction>)> {
        self.map.iter()
    }
}
