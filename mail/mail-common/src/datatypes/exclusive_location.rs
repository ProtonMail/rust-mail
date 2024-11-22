#[cfg(test)]
#[path = "../tests/datatypes/exclusive_location.rs"]
mod tests;

use crate::{
    datatypes::{LabelColor, LabelType, SystemLabel},
    models::Label,
};
use itertools::Itertools;
use proton_core_common::datatypes::{LabelId, LocalId};
use proton_core_common::models::ModelExtension;
use serde::{Deserialize, Serialize};
use stash::stash::{AgnosticInterface, Interface, StashError};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExclusiveLocation {
    System {
        name: SystemLabel,
        local_id: LocalId,
    },
    Custom {
        name: String,
        local_id: LocalId,
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
    /// # Parameters:
    /// * `label_ids` - list of label ids.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn from_label_ids<A>(
        label_ids: &[LabelId],
        interface: &A,
    ) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_ids = label_ids
            .iter()
            .map(|l| l.clone().into_inner())
            .collect_vec();
        let labels = Label::find_by_ids(label_ids, interface).await?;
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
