use crate::core::datatypes::Id;
use crate::mail::datatypes::{LabelColor, LabelDescription};
use proton_mail_common::datatypes::custom_folder::CustomFolder as RealCustomFolder;
use uniffi::Record as UniffiRecord;

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct CustomFolder {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub id: Id,

    /// TODO: Document this field.
    pub parent_id: Option<Id>,

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

impl From<RealCustomFolder> for CustomFolder {
    fn from(value: RealCustomFolder) -> Self {
        Self {
            id: value.local_id.into(),
            parent_id: value.parent_id.map(Into::into),
            children: value.children.into_iter().map(CustomFolder::from).collect(),
            color: value.color.map(LabelColor::from),
            display: value.display,
            expanded: value.expanded,
            label_description: value.label_description.into(),
            name: value.name,
            notify: value.notify,
            display_order: value.display_order,
            path: value.path,
            sticky: value.sticky,
            total: value.total,
            unread: value.unread,
        }
    }
}
