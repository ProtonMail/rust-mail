#![allow(clippy::module_inception)]

#[cfg(test)]
#[path = "../tests/models/labels_with_counters.rs"]
mod labels_with_counters;

use super::{ConversationCounter, MessageCounter};
use crate::models::MailSettings;
use indoc::formatdoc;
use proton_core_api::services::proton::{LabelId, ProtonCore};
use proton_core_common::datatypes::{LabelType, LocalLabelId};
use proton_core_common::models::{
    InitializationError, InitializationWatcher, InitializedComponent, Label, LabelError,
    ModelIdExtension,
};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::Row;
use stash::orm::{ConversionError, DbRecord};
use stash::stash::{Stash, WatcherHandle};
use stash::utils::{IterMapToSql, placeholders};
use stash::{
    exports::ToSql,
    orm::Model,
    params,
    stash::{StashError, Tether},
};
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct LabelWithCounters {
    pub label: Label,
    pub total_msg: u64,
    pub unread_msg: u64,
    pub total_conv: u64,
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
            move |tx, labels| {
                let label_ids = Label::store_labels(tx, labels)?;
                for local_id in label_ids {
                    ConversationCounter::new(local_id).save_sync(tx)?;
                    MessageCounter::new(local_id).save_sync(tx)?;
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
                    msgs = MessageCounter::table_name(),
                    convs = ConversationCounter::table_name(),
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
                    msgs = MessageCounter::table_name(),
                    convs = ConversationCounter::table_name(),
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
                    msgs = MessageCounter::table_name(),
                    convs = ConversationCounter::table_name(),
                ),
                label_ids,
            )
            .await?;

        Ok(values)
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(LabelWithCountersWatcher { sender }))
            .await
    }
}

impl DbRecord for LabelWithCounters {
    fn field_values(&self) -> impl Iterator<Item = &dyn ToSql> + '_ {
        unimplemented!("this model is read-only");

        #[allow(unused, reason = "false-positive")]
        [].into_iter()
    }

    fn from_row(row: &Row<'_>) -> Result<Self, ConversionError> {
        Ok(Self {
            label: Label::from_row(row)?,
            total_msg: row.get("total_msg")?,
            unread_msg: row.get("unread_msg")?,
            total_conv: row.get("total_conv")?,
            unread_conv: row.get("unread_conv")?,
        })
    }
}

impl Deref for LabelWithCounters {
    type Target = Label;

    fn deref(&self) -> &Self::Target {
        &self.label
    }
}

pub struct LabelWithCountersWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for LabelWithCountersWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            ConversationCounter::table_name().to_string(),
            MessageCounter::table_name().to_string(),
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
