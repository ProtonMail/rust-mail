#![allow(clippy::module_inception)]
#![allow(clippy::struct_excessive_bools)]

#[cfg(test)]
#[path = "../tests/models/labels.rs"]
mod labels;

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
use stash::exports::{Connection, Transaction};
use stash::macros::Model;
use stash::orm::{Model, ModelHooks};
use stash::params;
use stash::stash::{Bond, Stash, StashError, StashResult, Tether, WatcherHandle};
use stash::utils::{MapToSql as _, placeholders};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use topological_sort::TopologicalSort;
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
    pub const INIT_KEY: InitializationKey = InitializationKey::new("labels");

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

    pub async fn all_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Self::fetch_labels(api, &ALL_LABEL_TYPES).await
    }

    pub async fn fetch_mail_labels<API>(api: &API) -> Result<Vec<Label>, LabelError>
    where
        API: ProtonCore,
    {
        Self::fetch_labels(api, &MAIL_LABEL_TYPES).await
    }

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
        let labels = futures::future::join_all(
            label_types
                .iter()
                .map(|category| api.get_labels((*category).into())),
        )
        .await;

        let labels = labels
            .into_iter()
            .map(|res| {
                res.inspect_err(|err| error!("Failed to fetch labels: {err:?}"))
                    .map_err(LabelError::from)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|res| res.labels);

        Ok(Self::collect(labels))
    }

    fn collect(labels: impl IntoIterator<Item = ApiLabel>) -> Vec<Label> {
        let mut ids = TopologicalSort::<LabelId>::new();
        let mut objs = BTreeMap::new();

        for label in labels {
            if let Some(parent_id) = &label.parent_id {
                ids.add_dependency(parent_id.clone(), label.id.clone());
            } else {
                ids.insert(label.id.clone());
            }

            objs.insert(label.id.clone(), label);
        }

        // ---

        let mut labels = Vec::new();

        while let Some(id) = ids.pop() {
            if let Some(obj) = objs.remove(&id) {
                labels.push(obj.into());
            }
        }

        labels
    }

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

    pub async fn store_labels_async(
        tx: &Bond<'_>,
        labels: Vec<Label>,
    ) -> StashResult<Vec<LocalLabelId>> {
        tx.sync_bridge(move |tx| Self::store_labels(tx, labels))
            .await
    }

    pub fn store_labels(
        tx: &Transaction<'_>,
        labels: Vec<Label>,
    ) -> StashResult<Vec<LocalLabelId>> {
        let mut label_ids = Vec::with_capacity(labels.len());
        for mut label in labels {
            label.save_sync(tx)?;
            label_ids.push(label.id());
        }

        Ok(label_ids)
    }

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

    pub async fn find_by_kind(kind: LabelType, tether: &Tether) -> Result<Vec<Self>, StashError> {
        Label::find(
            "WHERE label_type = ? ORDER BY display_order ASC",
            params![kind],
            tether,
        )
        .await
    }

    pub async fn local_ids_by_kind(
        kind: LabelType,
        tether: &Tether,
    ) -> Result<Vec<LocalLabelId>, StashError> {
        Label::find_local_id_by(tether, "WHERE label_type = ?", params![kind]).await
    }

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

    pub async fn all_mail(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find_by_kinds(&MAIL_LABEL_TYPES, tether).await
    }

    pub async fn all_contact_groups(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find_by_kinds(&CONTACT_LABEL_TYPES, tether).await
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(LabelWatcher { sender }))
            .await
    }

    pub async fn resolve_remote_label_id(
        local_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<LabelId, LabelError> {
        let Some(label_id) = Self::local_id_counterpart(local_id, tether).await? else {
            return Err(LabelError::CouldNotResolveRemoteLabel(local_id));
        };

        Ok(label_id)
    }

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
    fn after_load(&mut self, conn: &Connection) -> Result<(), StashError> {
        if let Some(remote_id) = &self.remote_parent_id
            && self.local_parent_id.is_none()
        {
            self.local_parent_id = Self::remote_id_counterpart_sync(remote_id, conn)?;
        }
        // TODO: https://jira.protontech.ch/browse/ET-1169 ensure that local_remote_id are resolve for Label
        Ok(())
    }

    fn after_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        self.local_parent_id = match &self.remote_parent_id {
            Some(parent_id) => {
                let res = Self::remote_id_counterpart_sync(parent_id, tx)?;
                if res.is_none() {
                    error!(
                        ?self.local_id,
                        ?self.remote_id,
                        ?self.remote_parent_id,
                        "Got a label with a missing parent",
                    );
                }
                res
            }

            None => None,
        };

        tx.execute(
            "UPDATE labels SET local_parent_id=? WHERE local_id=?",
            (self.local_parent_id, self.local_id),
        )?;

        Ok(())
    }

    fn before_save(&mut self, tx: &Transaction<'_>) -> stash::stash::StashResult<()> {
        if let Some(remote_id) = &self.remote_id
            && let Some(label) = Label::find_first_sync("WHERE remote_id=?", (remote_id,), tx)?
        {
            self.local_parent_id = label.local_parent_id;
            self.local_id = label.local_id;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn api_label(id: &str, parent_id: Option<&str>) -> ApiLabel {
        ApiLabel {
            id: id.into(),
            parent_id: parent_id.map(Into::into),
            ..ApiLabel::test_default()
        }
    }

    #[test]
    fn collect() {
        let labels = Label::collect([
            api_label("a", None),
            api_label("b", Some("a")),
            api_label("c", Some("d")),
            api_label("d", None),
            api_label("e", Some("f")),
            api_label("f", Some("a")),
        ]);

        let labels: Vec<_> = labels
            .into_iter()
            .map(|label| label.remote_id.unwrap().to_string())
            .collect();

        // The topological-sort crate internally uses a hashmap, which means we
        // can't directly compare `labels` - we can only make sure the invariant
        // we're interested in is preserved
        let assert = |lhs: &str, ord: Ordering, rhs: &str| {
            let lhs_idx = labels.iter().find_position(|id| *id == lhs).unwrap();
            let rhs_idx = labels.iter().find_position(|id| *id == rhs).unwrap();

            assert_eq!(ord, lhs_idx.cmp(&rhs_idx));
        };

        // `b` depends on `a`, so `a` must be processed first
        assert("a", Ordering::Less, "b");

        // `c` depends on `d`, so `d` must be processed first
        assert("d", Ordering::Less, "c");

        // `e` depends on `f`, so `f` must be processed first
        assert("f", Ordering::Less, "e");

        // `f` depends on `a`, so `a` must be processed first
        assert("a", Ordering::Less, "f");
    }
}
