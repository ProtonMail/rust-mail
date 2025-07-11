#![allow(clippy::struct_excessive_bools)]

#[cfg(test)]
#[path = "../tests/models/labels.rs"]
mod labels;

use std::collections::BTreeSet;

use crate::datatypes::{
    ALL_LABEL_TYPES, CONTACT_LABEL_TYPES, InitializationKey, LabelColor, LabelType, LocalLabelId,
    MAIL_LABEL_TYPES,
};
use crate::models::ModelIdExtension;
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{PatchLabelRequest, PostLabelsRequest};
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::{Model, ModelHooks};
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use stash::utils::{MapToSql as _, placeholders};
use thiserror::Error;
use tracing::error;

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
    #[error("Label does not have neither remote nor local id")]
    LabelWithoutIds,
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[ModelHooks]
#[TableName("labels")]
pub struct Label {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalLabelId>,

    #[DbField]
    pub remote_id: Option<LabelId>,

    #[DbField]
    pub local_parent_id: Option<LocalLabelId>,

    #[DbField]
    pub remote_parent_id: Option<LabelId>,

    #[DbField]
    pub color: LabelColor,

    #[DbField]
    pub display: bool,

    #[DbField]
    pub expanded: bool,

    #[DbField]
    pub label_type: LabelType,

    #[DbField]
    pub name: String,

    #[DbField]
    pub notify: bool,

    #[DbField]
    pub display_order: u32,

    #[DbField]
    pub path: Option<String>,

    #[DbField]
    pub sticky: bool,
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
            }
        }

        <Self as Model>::save(self, bond).await
    }

    /// TODO: Document this function.
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
    /// # Errors
    ///
    /// Returns an error if the API request failed
    ///
    pub async fn all_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Self::fetch_labels(api, &ALL_LABEL_TYPES).await
    }

    /// Fetches mail labels from the API.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed
    ///
    pub async fn fetch_mail_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Self::fetch_labels(api, &MAIL_LABEL_TYPES).await
    }

    /// Fetches contact labels from the API.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed
    ///
    pub async fn fetch_contact_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Self::fetch_labels(api, &CONTACT_LABEL_TYPES).await
    }

    async fn fetch_labels<API>(
        api: &API,
        label_types: &[LabelType],
    ) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        let label_requests = futures::future::join_all(
            label_types
                .iter()
                .map(|category| api.get_labels((*category).into())),
        )
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
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn store_labels(
        tx: &Bond<'_>,
        labels: Vec<Label>,
    ) -> Result<Vec<LocalLabelId>, LabelError> {
        let mut label_ids = Vec::with_capacity(labels.len());
        for mut label in labels {
            label.save(tx).await?;
            let local_id = label.id();
            label_ids.push(local_id);
        }

        Ok(label_ids)
    }

    /// Function to update the label's expanded state in remote.
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

    /// Get all labels with given kinds
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    #[allow(trivial_casts)]
    pub async fn find_by_kinds(
        kinds: &[LabelType],
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let placeholders = placeholders(kinds);
        Label::find(
            format!("WHERE label_type IN ({placeholders}) ORDER BY display_order ASC"),
            kinds.to_sql(),
            tether,
        )
        .await
    }

    /// Get all mail labels
    ///
    pub async fn all_mail(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find_by_kinds(&MAIL_LABEL_TYPES, tether).await
    }

    /// Get all contact labels
    ///
    pub async fn all_contact_groups(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find_by_kinds(&CONTACT_LABEL_TYPES, tether).await
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

impl ModelHooks for Label {
    async fn after_load(&mut self, tether: &Tether) -> Result<(), StashError> {
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

    async fn after_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
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
        }
    }
}

impl Label {
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn test_default() -> Self {
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
        }
    }
}
