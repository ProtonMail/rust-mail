#![allow(clippy::module_inception)]

#[cfg(test)]
#[path = "../tests/models/labels_with_counters.rs"]
mod labels_with_counters;

use std::sync::Arc;

use indoc::formatdoc;
use proton_core_api::services::proton::{LabelId, ProtonCore};
use proton_core_common::datatypes::{LabelColor, LabelType, LocalLabelId};
use proton_core_common::models::{
    InitializationError, InitializationWatcher, InitializedComponent, Label, LabelError,
    ModelIdExtension,
};
use sqlite_watcher::watcher::TableObserver;
use stash::stash::{Stash, WatcherHandle};
use stash::utils::{IterMapToSql, placeholders};
use stash::{
    exports::ToSql,
    macros::DbRecord,
    orm::Model,
    params,
    stash::{StashError, Tether},
};

use super::{ConversationCounters, MessageCounters};

/// Helper data structure until we move from Stash to existing, mature ORM.
///
/// It loads both [`Label`] and [`MessageCounters`] with a single call. It is not only faster (because of the inner join)
/// but also easier to work with than separately with [`Label`] and [`MessageCounters`]
///
/// Note: It duplicates fields from [`Label`] since Stash does not support nested structures.
#[derive(DbRecord, PartialEq, Debug, Clone)]
pub struct LabelWithCounters {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[DbField]
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

    /// Number of total messages related to one particular label
    #[DbField]
    pub total_msg: u64,

    /// Number of unread messages related to one particular label
    #[DbField]
    pub unread_msg: u64,

    /// Number of total conversations related to one particular label
    #[DbField]
    pub total_conv: u64,

    /// Number of unread conversations related to one particular label
    #[DbField]
    pub unread_conv: u64,
}

impl LabelWithCounters {
    /// It initializes labels by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize<API>(
        watcher: Arc<InitializationWatcher>,
        api: &API,
        stash: &Stash,
    ) -> Result<(), InitializationError<LabelError>>
    where
        API: ProtonCore,
    {
        InitializedComponent::initialize::<LabelError, Vec<Label>>(
            watcher,
            Label::INIT_KEY,
            &[],
            stash.connection(),
            async || {
                let labels = Label::all_labels(api).await?;
                Ok(labels)
            },
            async |tx, labels| {
                let label_ids = Label::store_labels(tx, labels).await?;
                for local_id in label_ids {
                    ConversationCounters::new(local_id).save(tx).await?;
                    MessageCounters::new(local_id).save(tx).await?;
                }
                Ok(())
            },
        )
        .await
    }

    /// Performs INNER JOIN to load both resources at the same time.
    ///
    /// # Returns
    /// Maximum one row is returned. `Ok(None)` is returned if the database has no entry.
    ///
    /// # Errors
    /// It might return an error if the query fail
    pub async fn find_first(
        query: impl Into<String>,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let values = tether
            .query(
                formatdoc!(
                    "SELECT
                    {labels}.*,
                    {msgs}.total as total_msg,
                    {msgs}.unread as unread_msg,
                    {convs}.total as total_conv,
                    {convs}.unread as unread_conv
                FROM {labels}
                INNER JOIN {msgs}
                    ON {labels}.local_id = {msgs}.local_label_id
                INNER JOIN {convs}
                    ON {labels}.local_id = {convs}.local_label_id
                    {query}
                    LIMIT 1",
                    labels = Label::table_name(),
                    msgs = MessageCounters::table_name(),
                    convs = ConversationCounters::table_name(),
                    query = query.into()
                ),
                params,
            )
            .await?;

        Ok(values.into_iter().next())
    }

    /// Performs INNER JOIN to load both resources at the same time.
    ///
    /// # Returns
    /// Maximum one row is returned. `Ok(None)` is returned if the database has no entry.
    ///
    /// # Errors
    /// It might return an error if the query fail
    pub async fn load(label_id: LocalLabelId, tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::find_first(
            formatdoc!("WHERE {}.local_id = ?", Label::table_name()),
            params![label_id],
            tether,
        )
        .await
    }
    /// Performs INNER JOIN to load both resources at the same time.
    /// Filters by the [`LabelType`].
    ///
    /// # Returns
    /// Return Zero-Or-More values
    ///
    /// # Errors
    /// It might return an error if the query fail
    pub async fn find_by_kind(kind: LabelType, tether: &Tether) -> Result<Vec<Self>, StashError> {
        let values = tether
            .query(
                formatdoc!(
                    "SELECT
                {labels}.*,
                {msgs}.total as total_msg,
                {msgs}.unread as unread_msg,
                {convs}.total as total_conv,
                {convs}.unread as unread_conv
            FROM {labels}
            INNER JOIN {msgs}
                ON {labels}.local_id = {msgs}.local_label_id
            INNER JOIN {convs}
                ON {labels}.local_id = {convs}.local_label_id
            WHERE
                {labels}.label_type = ?
            ORDER BY
                {labels}.display_order ASC
            ",
                    labels = Label::table_name(),
                    msgs = MessageCounters::table_name(),
                    convs = ConversationCounters::table_name(),
                ),
                params![kind],
            )
            .await?;

        Ok(values)
    }

    /// Gets all system labels that are displayable
    pub async fn from_remote_ids(
        tether: &Tether,
        ids: impl IntoIterator<Item = LabelId>,
    ) -> anyhow::Result<Vec<Self>> {
        let ids = Label::remote_ids_counterpart(Vec::from_iter(ids), tether).await?;
        Self::from_ids(tether, ids).await
    }

    /// Gets all system labels that are displayable
    pub async fn from_ids(
        tether: &Tether,
        ids: impl IntoIterator<Item = LocalLabelId>,
    ) -> anyhow::Result<Vec<Self>> {
        // This is not suceptible to SQL injection since the labels are always numbers.
        let label_ids = ids.bridge_sql();
        let placeholders = placeholders(&label_ids);

        let values = tether
            .query(
                formatdoc!(
                    "SELECT
                {labels}.*,
                {msgs}.total as total_msg,
                {msgs}.unread as unread_msg,
                {convs}.total as total_conv,
                {convs}.unread as unread_conv
            FROM {labels}
            INNER JOIN {msgs}
                ON {labels}.local_id = {msgs}.local_label_id
            INNER JOIN {convs}
                ON {labels}.local_id = {convs}.local_label_id
            WHERE
                {labels}.local_id IN ({placeholders})
            ORDER BY
                {labels}.display_order ASC
            ",
                    labels = Label::table_name(),
                    msgs = MessageCounters::table_name(),
                    convs = ConversationCounters::table_name(),
                ),
                label_ids,
            )
            .await?;

        Ok(values)
    }

    pub fn label(&self) -> Label {
        let Self {
            local_id,
            remote_id,
            local_parent_id,
            remote_parent_id,
            color,
            display,
            expanded,
            label_type,
            name,
            notify,
            display_order,
            path,
            sticky,
            total_msg: _,
            unread_msg: _,
            total_conv: _,
            unread_conv: _,
        } = self.clone();
        Label {
            local_id,
            remote_id,
            local_parent_id,
            remote_parent_id,
            color,
            display,
            expanded,
            label_type,
            name,
            notify,
            display_order,
            path,
            sticky,
        }
    }

    /// Watch labels with counters for changes.
    ///
    /// When a change occurs a message is produced in the returned receiver.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed
    ///
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(LabelWithCountersWatcher { sender }))
    }
}

pub struct LabelWithCountersWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for LabelWithCountersWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            ConversationCounters::table_name().to_string(),
            MessageCounters::table_name().to_string(),
            Label::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _tables: &std::collections::BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for LabelWithCountersWatcher: {e:?}")
            })
            .ok();
    }
}
