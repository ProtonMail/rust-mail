use crate::datatypes::{ContextualLabel, LabelColor, LabelDescription};
use proton_core_common::datatypes::LocalId;

/// Contextual representation of a [`Label`] when it is opened for display.
#[derive(Clone, Debug)]
pub struct CustomFolder {
    /// Local id of the Label.
    pub local_id: LocalId,

    /// TODO: Document this field.
    pub parent_id: Option<LocalId>,

    /// List of the Labels contained in this Folder
    pub children: Vec<CustomFolder>,

    /// TODO: Document this field.
    pub color: Option<LabelColor>,

    /// TODO: Document this field.
    pub display: bool,

    /// TODO: Document this field.
    pub expanded: bool,

    /// TODO: Document this field.
    pub label_description: LabelDescription,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// TODO: Document this field.
    pub display_order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    pub sticky: bool,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

impl From<&ContextualLabel> for CustomFolder {
    fn from(value: &ContextualLabel) -> Self {
        Self {
            local_id: value.local_id,
            parent_id: value.parent_id,
            children: vec![],
            color: value.color.clone(),
            display: value.display,
            expanded: value.expanded,
            label_description: value.label_description,
            name: value.name.clone(),
            notify: value.notify,
            display_order: value.display_order,
            path: value.path.clone(),
            sticky: value.sticky,
            total: value.total,
            unread: value.unread,
        }
    }
}
