use super::IsSelected;
use crate::UniffiRecord;
use crate::mail::datatypes::{Id, LabelColor};
use proton_mail_common::actions::LabelAsAction as RealLabelAsAction;

/// This struct represents a label that can be used as an action.
///
#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct LabelAsAction {
    pub label_id: Id,
    pub name: String,
    pub color: LabelColor,
    pub order: u32,
    pub is_selected: IsSelected,
}

impl From<RealLabelAsAction> for LabelAsAction {
    fn from(value: RealLabelAsAction) -> Self {
        LabelAsAction {
            label_id: value.label_id.into(),
            name: value.name.clone(),
            color: value.color.into(),
            order: value.order,
            is_selected: IsSelected::new(value.is_selected),
        }
    }
}
