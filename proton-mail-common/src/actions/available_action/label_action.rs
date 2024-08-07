use crate::{datatypes::LabelColor, models::Label};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelAction {
    pub label_id: u64,
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
