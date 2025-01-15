#[cfg(test)]
#[path = "../tests/models/labels.rs"]
mod labels;

use std::collections::BTreeSet;

use crate::datatypes::{ConversationCount, MessageCount, SystemLabelId, ViewMode};
use crate::models::*;
use crate::AppError;
use indoc::formatdoc;
use itertools::Itertools;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::LabelId;
use proton_api_core::services::proton::requests::{
    PatchLabelRequest, PostLabelsRequest, PutLabelRequest,
};
use proton_api_core::services::proton::response_data::Label as ApiLabel;
use proton_api_core::services::proton::ProtonCore;
use proton_core_common::datatypes::{LabelColor, LabelType, LocalLabelId};
use proton_core_common::models::ModelIdExtension;
use proton_core_common::ALL_LABEL_TYPES;
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, error};

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
}

impl Label {
    /// Save or update a Label.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
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

    pub async fn create_or_update_conversation_counts(
        counts: Vec<ConversationCount>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for count in counts {
            bond.execute(
                formatdoc!(
                    r"
                    INSERT INTO conversation_counters(local_label_id, total, unread)
                    SELECT l.local_id, ?, ?
                    FROM labels AS l
                    WHERE l.remote_id = ?
                    ON CONFLICT(local_label_id) DO UPDATE
                    SET total = ?,
                        unread = ?
                    "
                ),
                params![
                    count.total,
                    count.unread,
                    count.label_id,
                    count.total,
                    count.unread
                ],
            )
            .await?;
        }
        Ok(())
    }

    pub async fn create_or_update_message_counts(
        counts: Vec<MessageCount>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for count in counts {
            bond.execute(
                formatdoc!(
                    r"
                    INSERT INTO message_counters(local_label_id, total, unread)
                    SELECT l.local_id, ?, ?
                        FROM labels AS l
                        WHERE l.remote_id = ?
                    ON CONFLICT(local_label_id) DO UPDATE
                        SET total = ?,
                            unread = ?
                    "
                ),
                params![
                    count.total,
                    count.unread,
                    count.label_id,
                    count.total,
                    count.unread
                ],
            )
            .await?;
        }
        Ok(())
    }

    /// TODO: Document this function.
    pub fn is_applicable_label(&self) -> bool {
        self.label_type == LabelType::Label || self.is_starred()
    }

    /// Checks if label is a System label - starred.
    pub fn is_starred(&self) -> bool {
        self.remote_id
            .as_ref()
            .is_some_and(|rid| *rid == LabelId::starred())
    }

    /// TODO: Document this function.
    pub fn is_movable_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self
                .remote_id
                .as_ref()
                .is_some_and(|rid| LabelId::movable_sys_folder_list().contains(rid))
    }

    /// Fetches all labels from the API and stores them in the database.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_labels<API: ProtonCore>(api: &API, stash: &Stash) -> Result<(), AppError> {
        let label_requests =
            futures::future::join_all(ALL_LABEL_TYPES.into_iter().map(|category| {
                debug!("Fetching labels ({:?})", category);
                api.get_labels(category.into())
            }))
            .await;

        debug!("Storing labels into database");
        let mut tether = stash.connection();
        let tx = tether.transaction().await?;
        for labels in label_requests {
            match labels {
                Err(e) => {
                    error!("Failed to fetch labels: {e}");
                    tx.commit().await?;
                    return Err(AppError::from(e));
                }
                Ok(labels) => {
                    for mut label in labels.labels.into_iter().map_into::<Self>() {
                        label.save(&tx).await?;
                        let local_id = label.local_id.unwrap();
                        ConversationCounters::new(local_id).save(&tx).await?;
                        MessageCounters::new(local_id).save(&tx).await?;
                    }
                }
            }
        }
        tx.commit().await?;

        Ok(())
    }

    /// Fetches the given labels from the API and stores them in the database.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_labels_by_ids<API: ProtonCore>(
        api: &API,
        tether: &mut Tether,
        ids: Vec<LabelId>,
    ) -> Result<(), AppError> {
        let labels = api
            .get_labels_by_ids(ids)
            .await?
            .labels
            .into_iter()
            .map_into::<Self>();

        debug!("Storing labels into database");
        let tx = tether.transaction().await?;
        for mut label in labels {
            Self::save(&mut label, &tx).await?;
        }
        tx.commit().await?;

        Ok(())
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

    /// Return the preferred view mode for this label.
    ///
    /// If this function returns [`None`] we should use the [`ViewMode`] defined
    /// in the user's [`MailSettings`], otherwise the returned value should be
    /// used.
    ///
    pub async fn view_mode(&self, tether: &Tether) -> Result<ViewMode, StashError> {
        if let Some(remote_id) = self.remote_id.as_ref() {
            if *remote_id == LabelId::drafts()
                || *remote_id == LabelId::sent()
                || *remote_id == LabelId::all_drafts()
                || *remote_id == LabelId::all_sent()
                || *remote_id == LabelId::all_scheduled()
            {
                return Ok(ViewMode::Messages);
            }
        }
        Ok(MailSettings::load(MAIL_SETTINGS_ID, tether)
            .await?
            .unwrap_or_default()
            .view_mode)
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
    ) -> Result<LabelId, AppError> {
        let Some(label_id) = Self::local_id_counterpart(local_id, tether).await? else {
            return Err(AppError::LabelNotFound(local_id));
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
    ) -> Result<LocalLabelId, AppError> {
        let Some(label_id) = Self::remote_id_counterpart(label_id.clone(), tether).await? else {
            return Err(AppError::RemoteLabelDoesNotExist(label_id));
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
            .inspect_err(|e| tracing::error!("Failed to send notification for LabelWatcher: {}", e))
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
            local_id: Default::default(),
            remote_id: Default::default(),
            local_parent_id: Default::default(),
            remote_parent_id: Default::default(),
            color: Default::default(),
            display: Default::default(),
            expanded: Default::default(),
            name: Default::default(),
            notify: Default::default(),
            display_order: Default::default(),
            path: Default::default(),
            sticky: Default::default(),
            row_id: Default::default(),
        }
    }
}
