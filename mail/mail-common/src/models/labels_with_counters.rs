#![allow(clippy::module_inception)]

#[cfg(test)]
#[path = "../tests/models/labels_with_counters.rs"]
mod labels_with_counters;

use super::{ConversationCounters, MessageCounters};
use crate::models::MailSettings;
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
use std::collections::BTreeSet;
use std::sync::Arc;

/// Helper data structure until we move from Stash to existing, mature ORM.
///
/// It loads both [`Label`] and [`MessageCounters`] with a single call. It is not only faster (because of the inner join)
/// but also easier to work with than separately with [`Label`] and [`MessageCounters`]
///
/// Note: It duplicates fields from [`Label`] since Stash does not support nested structures.
#[derive(DbRecord, PartialEq, Debug, Clone)]
pub struct LabelWithCounters {
    #[DbField]
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

    #[DbField]
    pub total_msg: u64,

    #[DbField]
    pub unread_msg: u64,

    #[DbField]
    pub total_conv: u64,

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
        InitializedComponent::initialize(
            watcher,
            Label::INIT_KEY,
            &[],
            stash.connection().await?,
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

    pub async fn load(label_id: LocalLabelId, tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::find_first(
            formatdoc!("WHERE {}.local_id = ?", Label::table_name()),
            params![label_id],
            tether,
        )
        .await
    }

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

    pub async fn from_remote_ids(
        tether: &Tether,
        ids: impl IntoIterator<Item = LabelId>,
    ) -> anyhow::Result<Vec<Self>> {
        let ids = Label::remote_ids_counterpart(Vec::from_iter(ids), tether).await?;

        Self::from_ids(tether, ids).await
    }

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
            MailSettings::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for LabelWithCountersWatcher: {e:?}")
            })
            .ok();
    }
}
