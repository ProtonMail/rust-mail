use crate::core::datatypes::Id;
use crate::mail::datatypes::{LabelColor, LabelDescription};
use proton_mail_common::datatypes::labels::custom_labels::CustomLabel as RealCustomLabel;
use uniffi::Record as UniffiRecord;

/// Represent a `Label` defined by End User
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct SidebarCustomLabel {
    /// Local id of the Label.
    pub id: Id,

    /// The color of the Label.
    pub color: LabelColor,

    /// Description of this Label.
    pub description: LabelDescription,

    /// TODO: Document this field.
    pub display: bool,

    /// The name of this Label.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// Order to display relative to other `CustomLabel`.
    pub display_order: u32,

    /// TODO: Document this field.
    pub sticky: bool,

    /// Total count of the message in this Label.
    pub total: u64,

    /// Count of unread message in this Label.
    pub unread: u64,
}

impl From<RealCustomLabel> for SidebarCustomLabel {
    fn from(value: RealCustomLabel) -> Self {
        Self {
            id: value.local_id.into(),
            color: value.color.into(),
            description: value.description.into(),
            display: value.display,
            name: value.name,
            notify: value.notify,
            display_order: value.display_order,
            sticky: value.sticky,
            total: value.total,
            unread: value.unread,
        }
    }
}
