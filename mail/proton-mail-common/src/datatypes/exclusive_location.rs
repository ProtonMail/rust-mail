#[cfg(test)]
#[path = "../tests/datatypes/exclusive_location.rs"]
mod tests;

use crate::{
    datatypes::{LabelColor, LabelType, SystemLabelId},
    models::Label,
};
use itertools::Itertools;
use lazy_static::lazy_static;
use proton_core_common::datatypes::LabelId;

lazy_static! {
    pub(crate) static ref INBOX_LABEL_ID: LabelId = LabelId::inbox();
    pub(crate) static ref TRASH_LABEL_ID: LabelId = LabelId::trash();
    pub(crate) static ref ARCHIVE_LABEL_ID: LabelId = LabelId::archive();
    pub(crate) static ref SPAM_LABEL_ID: LabelId = LabelId::spam();
    pub(crate) static ref SNOOZED_LABEL_ID: LabelId = LabelId::snoozed();
    pub(crate) static ref ALL_SCHEDULED_LABEL_ID: LabelId = LabelId::all_scheduled();
    pub(crate) static ref OUTBOX_LABEL_ID: LabelId = LabelId::outbox();
    pub(crate) static ref EXCLUSIVE_LOCATION_PRIORITY: Vec<&'static LabelId> = vec![
        &*INBOX_LABEL_ID,
        &*TRASH_LABEL_ID,
        &*ARCHIVE_LABEL_ID,
        &*SPAM_LABEL_ID,
        &*SNOOZED_LABEL_ID,
        &*ALL_SCHEDULED_LABEL_ID,
        &*OUTBOX_LABEL_ID,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExclusiveLocation {
    Inbox,
    Trash,
    Archive,
    Spam,
    Snoozed,
    Scheduled,
    Outbox,
    Custom {
        name: String,
        local_id: u64,
        color: LabelColor,
    },
}

impl ExclusiveLocation {
    pub fn new(label: &Label) -> Option<Self> {
        match Self::new_inner(label) {
            None => {
                tracing::error!(
                    "Could not get exclusive location from label lid: `{:?}`, rid: `{:?}`",
                    label.local_id,
                    label.remote_id
                );

                None
            }
            location => location,
        }
    }

    pub fn from_labels(labels: &[Label]) -> Option<Self> {
        let label = EXCLUSIVE_LOCATION_PRIORITY
            .iter()
            .find_map(|rid| find_label(labels, rid));

        match label {
            // Get a System Label.
            Some(label) => ExclusiveLocation::new(label),
            // Otherwise try to get a custom Folder.
            None => match labels
                .iter()
                .filter_map(ExclusiveLocation::new)
                .exactly_one()
            {
                Ok(location) => Some(location),
                Err(e) => {
                    tracing::error!("Error while getting exclusive location: {e}");
                    None
                }
            },
        }
    }

    fn new_inner(label: &Label) -> Option<Self> {
        match label.label_type {
            LabelType::Label | LabelType::ContactGroup => None,
            LabelType::System => match label.remote_id.as_ref()? {
                x if x == &*INBOX_LABEL_ID => Some(ExclusiveLocation::Inbox),
                x if x == &*TRASH_LABEL_ID => Some(ExclusiveLocation::Trash),
                x if x == &*ARCHIVE_LABEL_ID => Some(ExclusiveLocation::Archive),
                x if x == &*SPAM_LABEL_ID => Some(ExclusiveLocation::Spam),
                x if x == &*SNOOZED_LABEL_ID => Some(ExclusiveLocation::Snoozed),
                x if x == &*ALL_SCHEDULED_LABEL_ID => Some(ExclusiveLocation::Scheduled),
                x if x == &*OUTBOX_LABEL_ID => Some(ExclusiveLocation::Outbox),
                _ => None,
            },
            LabelType::Folder => Some(ExclusiveLocation::Custom {
                name: label.name.clone(),
                local_id: label.local_id?,
                color: label.color.clone(),
            }),
        }
    }
}

fn find_label<'a>(labels: &'a [Label], rid: &LabelId) -> Option<&'a Label> {
    labels
        .iter()
        .find(|label| label.remote_id.as_ref().map(|r| r == rid).unwrap_or(false))
}
