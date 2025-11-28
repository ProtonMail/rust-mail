use crate::datatypes::LabelType;
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::Label;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(u8)]
pub enum MovableSystemFolder {
    Inbox = 0,
    Trash = 3,
    Spam = 4,
    Archive = 6,
}

impl MovableSystemFolder {
    pub(crate) fn new(label: &Label) -> Option<Self> {
        match label.label_type {
            LabelType::Label | LabelType::ContactGroup | LabelType::Folder | LabelType::System => {
                Self::from_rid(label.remote_id.as_ref())
            }
        }
    }

    fn from_rid(label_id: Option<&LabelId>) -> Option<Self> {
        let remote_id: u8 = label_id?.parse().ok()?;

        match remote_id {
            x if x == Self::Inbox as u8 => Some(Self::Inbox),
            x if x == Self::Trash as u8 => Some(Self::Trash),
            x if x == Self::Spam as u8 => Some(Self::Spam),
            x if x == Self::Archive as u8 => Some(Self::Archive),
            _ => None,
        }
    }
}
