use crate::{datatypes::LabelColor, models::Label};
use proton_core_common::datatypes::LocalId;

#[derive(Debug, Clone, PartialEq)]
pub struct LabelAction {
    pub label_id: LocalId,
    pub name: String,
    pub color: LabelColor,
}

impl LabelAction {
    pub fn from_label(label: &Label) -> Option<Self> {
        Some(LabelAction {
            label_id: label.local_id?,
            name: label.name.clone(),
            color: label.color.clone(),
        })
    }
}
