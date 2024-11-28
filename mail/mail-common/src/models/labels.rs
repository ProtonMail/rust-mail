#[cfg(test)]
#[path = "../tests/models/labels.rs"]
mod labels;

use crate::datatypes::{
    ConversationCount, LabelColor, LabelType, MessageCount, SystemLabelId, ViewMode,
};
use crate::models::*;
use crate::{AppError, ALL_LABEL_TYPES};
use indoc::formatdoc;
use itertools::Itertools;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::requests::{
    PatchLabelRequest, PostLabelsRequest, PutLabelRequest,
};
use proton_api_mail::services::proton::response_data::{Label as ApiLabel, OperationResult};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LabelId, LocalId};
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};
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
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_parent_id: Option<LocalId>,

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
    pub initialized_conv: bool,

    /// TODO: Document this field.
    #[DbField]
    pub initialized_msg: bool,

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

    /// TODO: Document this field.
    #[DbField]
    pub total_conv: u64,

    /// TODO: Document this field.
    #[DbField]
    pub total_msg: u64,

    /// TODO: Document this field.
    #[DbField]
    pub unread_conv: u64,

    /// TODO: Document this field.
    #[DbField]
    pub unread_msg: u64,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
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
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update a Label.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(label) =
                Label::find_first("WHERE remote_id=?", params![remote_id], interface).await?
            {
                self.local_parent_id = label.local_parent_id;
                self.local_id = label.local_id;
                self.row_id = label.row_id;
                self.stash = label.stash;
            }
        }

        <Self as Model>::save_using(self, interface).await
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
    pub async fn create<PM: ProtonMail>(
        name: String,
        color: String,
        label_type: LabelType,
        parent_id: Option<LabelId>,
        api: &PM,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .post_labels(PostLabelsRequest {
                parent_id: parent_id.map(|id| id.into()),
                color,
                label_type: label_type.into(),
                name,
            })
            .await?
            .label
            .into())
    }

    pub async fn create_or_update_conversation_counts<A>(
        counts: Vec<ConversationCount>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for count in counts {
            interface
                .execute(
                    formatdoc!(
                        r"
                    UPDATE
                        labels
                    SET
                        total_conv = ?,
                        unread_conv = ?
                    WHERE
                        remote_id = ?
                    "
                    ),
                    params![count.total, count.unread, count.label_id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn create_or_update_message_counts<A>(
        counts: Vec<MessageCount>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for count in counts {
            interface
                .execute(
                    formatdoc!(
                        r"
                    UPDATE
                        labels
                    SET
                        total_msg = ?,
                        unread_msg = ?
                    WHERE
                        remote_id = ?
                    "
                    ),
                    params![count.total, count.unread, count.label_id],
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
            .as_ref().is_some_and(|rid| *rid == LabelId::starred())
    }

    /// TODO: Document this function.
    pub fn is_movable_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self.remote_id.as_ref().is_some_and(|rid| {
                LabelId::movable_sys_folder_list().contains(rid)
            })
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
    pub async fn sync_labels<PM: ProtonMail, A>(api: &PM, interface: &A) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_requests =
            futures::future::join_all(ALL_LABEL_TYPES.into_iter().map(|category| {
                debug!("Fetching labels ({:?})", category);
                api.get_labels(category.into())
            }))
            .await;

        debug!("Storing labels into database");
        let tx = interface.transaction().await?;
        for labels in label_requests {
            match labels {
                Err(e) => {
                    error!("Failed to fetch labels: {e}");
                    tx.commit().await?;
                    return Err(AppError::from(e));
                }
                Ok(labels) => {
                    for mut label in labels.labels.into_iter().map_into::<Self>() {
                        label.save_using(&tx).await?;
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
    pub async fn sync_labels_by_ids<PM: ProtonMail, A>(
        api: &PM,
        interface: &A,
        ids: Vec<ApiRemoteId>,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let labels = api
            .get_labels_by_ids(ids)
            .await?
            .labels
            .into_iter()
            .map_into::<Self>();

        debug!("Storing labels into database");
        let tx = interface.transaction().await?;
        for mut label in labels {
            Self::save_using(&mut label, &tx).await?;
        }
        tx.commit().await?;

        Ok(())
    }

    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        if self.remote_parent_id.is_some() && self.local_parent_id.is_none() {
            self.local_parent_id = self
                .remote_parent_id
                .clone()
                .expect("Should be set")
                .counterpart::<Self, _>(interface)
                .await?;
        }
        // TODO: https://jira.protontech.ch/browse/ET-1169 ensure that local_remote_id are resolve for Label
        Ok(())
    }

    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        let parent_id_option = self.remote_parent_id.clone();
        self.local_parent_id = match parent_id_option {
            Some(parent_id) => {
                let res = parent_id.counterpart::<Self, _>(interface).await?;
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
        interface
            .execute(
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
    pub async fn update<PM: ProtonMail>(
        id: LabelId,
        name: String,
        color: String,
        parent_id: Option<LabelId>,
        api: &PM,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .put_label(
                id.into(),
                PutLabelRequest {
                    parent_id: parent_id.map(|id| id.into()),
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
    pub async fn patch_expanded<PM: ProtonMail>(
        id: LabelId,
        expanded: bool,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        api.patch_label(
            id.into(),
            PatchLabelRequest {
                expanded: Some(expanded),
                ..Default::default()
            },
        )
        .await
        .map(|r| r.responses)
    }

    /// Return the preferred view mode for this label.
    ///
    /// If this function returns [`None`] we should use the [`ViewMode`] defined
    /// in the user's [`MailSettings`], otherwise the returned value should be
    /// used.
    ///
    pub async fn view_mode<A>(&self, interface: &A) -> Result<ViewMode, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
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
        Ok(MailSettings::load(MAIL_SETTINGS_ID.into(), interface)
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
    pub async fn find_by_kind<A>(kind: LabelType, interface: &A) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Label::find(
            "WHERE label_type = ? ORDER BY display_order ASC",
            params![kind],
            interface,
            None,
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
    pub async fn watch<A>(
        local_id: LocalId,
        interface: &A,
    ) -> Result<
        Option<(
            Self,
            flume::Receiver<ResultsetChange<Self, <Self as Model>::IdType>>,
        )>,
        AppError,
    >
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (sender, receiver) = flume::unbounded();
        let mut labels = Label::find(
            "WHERE local_id=?",
            params![local_id],
            interface,
            Some(sender),
        )
        .await?;
        if labels.is_empty() {
            return Ok(None);
        }

        Ok(Some((labels.swap_remove(0), receiver)))
    }

    /// Resolve the remote id for a label with `local_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_remote_label_id<A>(
        local_id: LocalId,
        interface: &A,
    ) -> Result<LabelId, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(label_id) = local_id.counterpart::<Label, _>(interface).await? else {
            return Err(AppError::LabelNotFound(local_id));
        };

        Ok(label_id.into())
    }

    /// Resolve the local id for a label with `label_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_local_label_id<A>(
        label_id: LabelId,
        interface: &A,
    ) -> Result<LocalId, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(label_id) = label_id.counterpart::<Label, _>(interface).await? else {
            return Err(AppError::RemoteLabelDoesNotExist(label_id));
        };
        Ok(label_id)
    }
}

impl From<ApiLabel> for Label {
    fn from(value: ApiLabel) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            local_parent_id: None,
            remote_parent_id: value.parent_id.map(|id| id.into()),
            color: value.color.into(),
            display_order: value.order,
            display: value.display,
            expanded: value.expanded,
            initialized_conv: false,
            initialized_msg: false,
            label_type: value.label_type.into(),
            name: value.name,
            notify: value.notify,
            path: value.path,
            sticky: value.sticky,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: None,
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
            initialized_conv: Default::default(),
            initialized_msg: Default::default(),
            name: Default::default(),
            notify: Default::default(),
            display_order: Default::default(),
            path: Default::default(),
            sticky: Default::default(),
            total_conv: Default::default(),
            total_msg: Default::default(),
            unread_conv: Default::default(),
            unread_msg: Default::default(),
            row_id: Default::default(),
            stash: Default::default(),
        }
    }
}
