use crate::core::datatypes::Id;
use crate::mail::datatypes::{LabelColor, LabelDescription};
use proton_mail_common::datatypes::labels::custom_folder::CustomFolder as RealCustomFolder;
use uniffi::Record as UniffiRecord;

/// Contextual representation of a `Label` when it is opened for display.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct SidebarCustomFolder {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub id: Id,

    /// Id of the parent `Folder` of this `Folder` if any.
    pub parent_id: Option<Id>,

    /// List of the Labels contained in this Folder
    pub children: Vec<SidebarCustomFolder>,

    /// Color to display this `Folder` with.
    pub color: Option<LabelColor>,

    /// Description of this `Folder`.
    pub description: LabelDescription,

    /// TODO: Document this field.
    pub display: bool,

    /// Is this `Folder` expanded?
    pub expanded: bool,

    /// Name of this `Folder`.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// Order to display all the `Folders`.
    pub display_order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    pub sticky: bool,

    /// Total number of `Messages` in this `Folder`.
    pub total: u64,

    /// Numer of unread `Messages` in this `FOlder`.
    pub unread: u64,
}

impl From<RealCustomFolder> for SidebarCustomFolder {
    fn from(value: RealCustomFolder) -> Self {
        Self {
            id: value.local_id.into(),
            parent_id: value.parent_id.map(Into::into),
            children: value
                .children
                .into_iter()
                .map(SidebarCustomFolder::from)
                .collect(),
            color: value.color.map(LabelColor::from),
            display: value.display,
            expanded: value.expanded,
            description: value.description.into(),
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
