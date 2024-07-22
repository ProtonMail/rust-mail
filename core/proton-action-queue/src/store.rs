#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;

use crate::{Action, ActionId, ActionPriority};
use proton_sqlite3::{rusqlite, Migration, MigratorError};
use rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::datatypes::QueryResultU64;
use stash::macros::Model;
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use std::fmt::{Debug, Display, Formatter};
use tracing::{debug, span, Level};

/// Id of an action stored in the queue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct StoredActionId(pub u64);

impl Display for StoredActionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromSql for StoredActionId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u64::column_result(value).map(StoredActionId)
    }
}

impl ToSql for StoredActionId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

/// Represents a stored action.
#[derive(Clone, Debug, Deserialize, Model, PartialEq, Serialize)]
#[TableName("action_queue")]
pub struct StoredAction {
    #[IdField]
    pub id: StoredActionId,
    #[DbField]
    pub action_id: ActionId,
    #[DbField]
    pub version: u32,
    #[DbField]
    pub date_time: chrono::DateTime<chrono::Utc>,
    #[DbField]
    pub priority: ActionPriority,
    #[DbField]
    data: Vec<u8>,
    #[RowIdField]
    #[serde(skip)]
    row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    stash: Option<Stash>,
}

#[doc(hidden)]
pub(crate) struct PendingAction {
    action_id: ActionId,
    version: u32,
    priority: ActionPriority,
    data: Vec<u8>,
}

impl StoredAction {
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&self.data)
    }
}

impl PendingAction {
    pub(crate) fn from_action<T: Action>(action: &T) -> Result<Self, rmp_serde::encode::Error> {
        let data = rmp_serde::to_vec(action)?;

        Ok(Self {
            action_id: action.action_id().clone(),
            version: action.action_version(),
            priority: action.priority(),
            data,
        })
    }
}

pub struct ActionStore(Tether);

const ACTION_VERSION_TABLE_NAME: &str = "action_queue_version";
const ACTION_TABLE_NAME: &str = "action_queue";
const ACTION_TABLE_FIELD_ID: &str = "id";
const ACTION_TABLE_FIELD_VERSION: &str = "version";
const ACTION_TABLE_FIELD_ACTION_ID: &str = "action_id";
const ACTION_TABLE_FIELD_PRIORITY: &str = "priority";
const ACTION_TABLE_FIELD_DATA: &str = "data";
const ACTION_TABLE_FIELD_DATE_TIME: &str = "date_time";

const ACTION_TABLE_PRIORITY_INDEX_NAME: &str = "action_queue_priority_index";
const ACTION_TABLE_DATE_TIME_INDEX_NAME: &str = "action_queue_date_time_index";

impl ActionStore {
    pub fn new(tx: Tether) -> Self {
        Self(tx)
    }

    pub async fn init_tables(stash: &Stash) -> Result<(), MigratorError> {
        let span = span!(Level::DEBUG, "Action Table Setup");
        {
            let _entered = span.enter();
            let migrator = proton_sqlite3::Migrator::new();
            let migrations = vec![ActionTableMigrationV1 {}];

            let version = migrator
                .migrate(stash, ACTION_VERSION_TABLE_NAME, &migrations)
                .await?;
            debug!("Current version={version}");
            Ok(())
        }
    }

    pub async fn get_next_action(&mut self) -> Result<Option<StoredAction>, StashError> {
        let query = format!(
            "SELECT rowid AS rowid, {ACTION_TABLE_FIELD_ID}, {ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, \
{ACTION_TABLE_FIELD_DATE_TIME}, {ACTION_TABLE_FIELD_DATA} FROM {ACTION_TABLE_NAME} \
ORDER BY {ACTION_TABLE_FIELD_PRIORITY} ASC ,{ACTION_TABLE_FIELD_DATE_TIME} ASC"
        );

        Ok(self
            .0
            .query::<_, StoredAction>(query, vec![])
            .await?
            .into_iter()
            .next())
    }
    pub async fn get_stored_actions(&mut self) -> Result<Vec<StoredAction>, StashError> {
        let query = format!(
            "SELECT rowid AS rowid, {ACTION_TABLE_FIELD_ID}, {ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, \
{ACTION_TABLE_FIELD_DATE_TIME}, {ACTION_TABLE_FIELD_DATA} FROM {ACTION_TABLE_NAME}"
        );
        let actions = self.0.query::<_, StoredAction>(query, vec![]).await?;

        let mut constructed = Vec::new();
        for action in actions {
            constructed.push(action)
        }

        Ok(constructed)
    }

    pub(crate) async fn store_action(
        &mut self,
        action: PendingAction,
    ) -> Result<StoredActionId, StashError> {
        let result = self.store_actions(&[action]).await?;
        Ok(result[0])
    }

    pub(crate) async fn store_actions(
        &mut self,
        actions: &[PendingAction],
    ) -> Result<Vec<StoredActionId>, StashError> {
        let query = format!("INSERT INTO {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, {ACTION_TABLE_FIELD_DATA}) VALUES (?, ?, ?, ?)\
RETURNING {ACTION_TABLE_FIELD_ID} AS value");
        let mut ids = Vec::with_capacity(actions.len());
        for action in actions {
            let id = self
                .0
                .query::<_, QueryResultU64>(
                    &query,
                    vec![
                        Box::new(action.version),
                        Box::new(action.action_id.clone()),
                        Box::new(action.priority),
                        Box::new(action.data.clone()),
                    ],
                )
                .await?
                .first()
                .unwrap()
                .value;
            ids.push(StoredActionId(id));
        }
        Ok(ids)
    }

    pub async fn erase_actions(&mut self, action_ids: &[StoredActionId]) -> Result<(), StashError> {
        let query = format!("DELETE FROM {ACTION_TABLE_NAME} WHERE {ACTION_TABLE_FIELD_ID}=?");

        for id in action_ids {
            self.0.execute(&query, params![id.0]).await?;
        }

        Ok(())
    }

    pub fn tx(&self) -> Tether {
        self.0.clone()
    }
}

struct ActionTableMigrationV1 {}

impl Migration for ActionTableMigrationV1 {
    fn name(&self) -> &str {
        "action_table_v1"
    }

    async fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
        // create actions table
        let query = format!(
            "CREATE TABLE {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_ID} \
INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, {ACTION_TABLE_FIELD_ACTION_ID} BLOB NOT NULL, \
{ACTION_TABLE_FIELD_VERSION} INTEGER NOT NULL, {ACTION_TABLE_FIELD_PRIORITY} INTEGER NOT NULL, \
{ACTION_TABLE_FIELD_DATE_TIME} INTEGER DEFAULT (datetime('now')), \
{ACTION_TABLE_FIELD_DATA} BLOB NOT NULL)"
        );
        tx.execute(query, vec![]).await?;

        // Create index on Priority & Date
        let query= format!("CREATE INDEX {ACTION_TABLE_PRIORITY_INDEX_NAME} ON {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_PRIORITY})");
        tx.execute(query, vec![]).await?;

        let query= format!("CREATE INDEX {ACTION_TABLE_DATE_TIME_INDEX_NAME} ON {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_DATE_TIME})");
        tx.execute(query, vec![]).await?;

        Ok(())
    }
}
