#![allow(clippy::struct_excessive_bools)]

#[cfg(test)]
#[path = "../tests/models/labels.rs"]
mod labels;

use std::collections::BTreeSet;

use crate::datatypes::{ALL_LABEL_TYPES, InitializationKey, LabelColor, LabelType, LocalLabelId};
use crate::models::ModelIdExtension;
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{PatchLabelRequest, PostLabelsRequest, PutLabelRequest};
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Error)]
pub enum LabelError {
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),

    #[error("Could not resolve remote label: {0}")]
    CouldNotResolveRemoteLabel(LocalLabelId),
    #[error("Could not resolve local label: {0}")]
    CouldNotResolveLocalLabel(LabelId),
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[ModelActions(on_load, on_save)]
#[TableName("labels")]
pub struct Label {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalLabelId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_parent_id: Option<LocalLabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_parent_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub color: LabelColor,

    /// TODO: Document this field.
    #[DbField]
    pub display: bool,

    /// TODO: Document this field.
    #[DbField]
    pub expanded: bool,

    /// TODO: Document this field.
    #[DbField]
    pub label_type: LabelType,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    /// TODO: Document this field.
    #[DbField]
    pub notify: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub path: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub sticky: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl ModelIdExtension for Label {
    type RemoteId = LabelId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

impl Label {
    /// Key used to distinguish between components in the initialization.
    /// It is a string, not an enum for making it open for additional changes from different BU.
    ///
    pub const INIT_KEY: InitializationKey = InitializationKey::new("labels");

    /// Save or update a Label.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// `label_id` is not set, the local label can not be found or the query
    /// failed.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(label) =
                Label::find_first("WHERE remote_id=?", params![remote_id], bond).await?
            {
                self.local_parent_id = label.local_parent_id;
                self.local_id = label.local_id;
                self.row_id = label.row_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }

    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `name`       - TODO: Document this parameter.
    /// * `color`      - TODO: Document this parameter.
    /// * `label_type` - TODO: Document this parameter.
    /// * `parent_id`  - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn create<API: ProtonCore>(
        name: String,
        color: String,
        label_type: LabelType,
        parent_id: Option<LabelId>,
        api: &API,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .post_labels(PostLabelsRequest {
                parent_id,
                color,
                label_type: label_type.into(),
                name,
            })
            .await?
            .label
            .into())
    }

    /// Fetches all labels from the API.
    ///
    /// # Parameters
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed
    ///
    pub async fn all_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        let label_requests =
            futures::future::join_all(ALL_LABEL_TYPES.into_iter().map(|category| {
                debug!("Fetching labels ({:?})", category);
                api.get_labels(category.into())
            }))
            .await;

        Ok(label_requests
            .into_iter()
            .map(|res| {
                res.inspect_err(|err| error!("Failed to fetch labels: {err:?}"))
                    .map_err(LabelError::from)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|res| res.labels)
            .map_into::<Self>()
            .collect())
    }

    /// Fetches the given labels from the API.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn get_labels_by_ids<API>(
        api: &API,
        ids: Vec<LabelId>,
    ) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Ok(api
            .get_labels_by_ids(ids)
            .await?
            .labels
            .into_iter()
            .map_into::<Self>()
            .collect())
    }

    /// Stores given labels in the database.
    ///
    /// # Parameters
    ///
    /// * `tx` - The stash transaction to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    /// # Panics
    /// If labels fetched from database do not contain Local ID.
    /// Note, this is rather impossible and is just a matter of limitations of Stash API
    ///
    pub async fn sync_labels(
        tx: &Bond<'_>,
        labels: Vec<Label>,
    ) -> Result<Vec<LocalLabelId>, LabelError> {
        debug!("Storing labels into database");
        let mut label_ids = Vec::with_capacity(labels.len());
        for mut label in labels {
            label.save(tx).await?;
            let local_id = label.local_id.unwrap();
            label_ids.push(local_id);
        }

        Ok(label_ids)
    }

    async fn on_load(&mut self, tether: &Tether) -> Result<(), StashError> {
        if self.remote_parent_id.is_some() && self.local_parent_id.is_none() {
            self.local_parent_id = Self::remote_id_counterpart(
                self.remote_parent_id.clone().expect("Should be set"),
                tether,
            )
            .await?;
        }
        // TODO: https://jira.protontech.ch/browse/ET-1169 ensure that local_remote_id are resolve for Label
        Ok(())
    }

    pub async fn on_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let parent_id_option = self.remote_parent_id.clone();
        self.local_parent_id = match parent_id_option {
            Some(parent_id) => {
                let res = Self::remote_id_counterpart(parent_id, bond).await?;
                if res.is_none() {
                    // TODO: handle this error
                    error!(
                        "A Label({:?}) remote_parent don't have corresponding local_id",
                        self.remote_id
                    );
                }
                res
            }
            None => None,
        };
        bond.execute(
            format!(
                "UPDATE {} SET local_parent_id=? WHERE local_id=?",
                Label::table_name()
            ),
            params![self.local_parent_id, self.local_id],
        )
        .await?;
        Ok(())
    }

    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `id`         - The ID of the label to update.
    /// * `name`       - TODO: Document this parameter.
    /// * `color`      - TODO: Document this parameter.
    /// * `label_type` - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn update<API: ProtonCore>(
        id: LabelId,
        name: String,
        color: String,
        parent_id: Option<LabelId>,
        api: &API,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .put_label(
                id,
                PutLabelRequest {
                    parent_id,
                    color,
                    name,
                },
            )
            .await?
            .label
            .into())
    }

    /// Function to update the label's expanded state in remote.
    ///
    /// # Parameters
    ///
    /// * `id`         - The Remote ID of the label to update.
    /// * `expanded`   - The new expanded state.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn patch_expanded<API: ProtonCore>(
        id: LabelId,
        expanded: bool,
        api: &API,
    ) -> Result<Label, ApiServiceError> {
        api.patch_label(
            id,
            PatchLabelRequest {
                expanded: Some(expanded),
                ..Default::default()
            },
        )
        .await
        .map(|r| r.label.into())
    }

    /// Get all labels with given kind
    ///
    /// # Parameters
    ///
    /// * `kind` - The kind of the label, eg. System, Folder etc.
    /// * `tx`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_by_kind(kind: LabelType, tether: &Tether) -> Result<Vec<Self>, StashError> {
        Label::find(
            "WHERE label_type = ? ORDER BY display_order ASC",
            params![kind],
            tether,
        )
        .await
    }

    /// Watch a label with the given `local_id` for changes.
    ///
    /// When a change occurs a message is produced in the returned receiver.
    ///
    /// Returns `None` if the label was not found.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(LabelWatcher { sender }))
    }

    /// Resolve the remote id for a label with `local_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_remote_label_id(
        local_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<LabelId, LabelError> {
        let Some(label_id) = Self::local_id_counterpart(local_id, tether).await? else {
            return Err(LabelError::CouldNotResolveRemoteLabel(local_id));
        };

        Ok(label_id)
    }

    /// Resolve the local id for a label with `label_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_local_label_id(
        label_id: LabelId,
        tether: &Tether,
    ) -> Result<LocalLabelId, LabelError> {
        let Some(label_id) = Self::remote_id_counterpart(label_id.clone(), tether).await? else {
            return Err(LabelError::CouldNotResolveLocalLabel(label_id));
        };
        Ok(label_id)
    }
}

pub struct LabelWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for LabelWatcher {
    fn tables(&self) -> Vec<String> {
        vec![Label::table_name().to_string()]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| error!("Failed to send notification for LabelWatcher: {e:?}"))
            .ok();
    }
}

impl From<ApiLabel> for Label {
    fn from(value: ApiLabel) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            local_parent_id: None,
            remote_parent_id: value.parent_id,
            color: value.color.into(),
            display_order: value.order,
            display: value.display,
            expanded: value.expanded,
            label_type: value.label_type.into(),
            name: value.name,
            notify: value.notify,
            path: value.path,
            sticky: value.sticky,
            row_id: None,
        }
    }
}

impl Default for Label {
    fn default() -> Self {
        Self {
            label_type: LabelType::Label,
            local_id: Option::default(),
            remote_id: Option::default(),
            local_parent_id: Option::default(),
            remote_parent_id: Option::default(),
            color: LabelColor::default(),
            display: Default::default(),
            expanded: Default::default(),
            name: String::default(),
            notify: Default::default(),
            display_order: Default::default(),
            path: Option::default(),
            sticky: Default::default(),
            row_id: Option::default(),
        }
    }
}
