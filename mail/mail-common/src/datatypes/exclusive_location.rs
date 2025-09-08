#[cfg(test)]
#[path = "../tests/datatypes/exclusive_location.rs"]
mod tests;

use crate::datatypes::{LabelColor, LabelType};
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::ModelIdExtension;
use proton_core_common::{datatypes::LocalLabelId, models::Label};
use serde::{Deserialize, Serialize};
use stash::exports::Connection;
use stash::stash::{StashError, Tether};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExclusiveLocation {
    System {
        name: SystemLabel,
        local_id: LocalLabelId,
    },
    Custom {
        name: String,
        local_id: LocalLabelId,
        color: LabelColor,
    },
}

impl ExclusiveLocation {
    pub fn new(label: &Label) -> Option<Self> {
        match Self::new_inner(label) {
            None => {
                tracing::trace!(
                    "Could not get exclusive location from label lid: `{:?}`, rid: `{:?}`",
                    label.local_id,
                    label.remote_id
                );

                None
            }
            location => location,
        }
    }

    pub fn local_id(&self) -> LocalLabelId {
        match self {
            ExclusiveLocation::System { local_id, .. }
            | ExclusiveLocation::Custom { local_id, .. } => *local_id,
        }
    }

    pub fn from_labels(labels: &[Label]) -> Option<Self> {
        let label = SystemLabel::exclusive_locations()
            .into_iter()
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
                    tracing::trace!("Error while getting exclusive location: {e}");
                    None
                }
            },
        }
    }

    /// Calculate exclusive location from a list of label ids.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn from_label_ids(
        label_ids: &[LabelId],
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let labels = Label::find_by_remote_ids(label_ids.iter().cloned(), tether).await?;
        Ok(ExclusiveLocation::from_labels(&labels))
    }

    pub fn from_label_ids_sync(
        label_ids: &[LabelId],
        conn: &Connection,
    ) -> Result<Option<Self>, StashError> {
        let labels = Label::find_by_remote_ids_sync(label_ids, conn)?;
        Ok(ExclusiveLocation::from_labels(&labels))
    }

    fn new_inner(label: &Label) -> Option<Self> {
        match label.label_type {
            LabelType::Label | LabelType::ContactGroup => None,
            LabelType::System => {
                let system_label = SystemLabel::new(label)?;

                if system_label.is_exclusive_location() {
                    Some(ExclusiveLocation::System {
                        name: system_label,
                        local_id: label.local_id?,
                    })
                } else {
                    None
                }
            }
            LabelType::Folder => Some(ExclusiveLocation::Custom {
                name: label.name.clone(),
                local_id: label.local_id?,
                color: label.color.clone(),
            }),
        }
    }
}

fn find_label(labels: &[Label], rid: SystemLabel) -> Option<&Label> {
    labels.iter().find(|label| {
        label
            .remote_id
            .as_ref()
            .map(|r| r == &rid.into())
            .unwrap_or(false)
    })
}
