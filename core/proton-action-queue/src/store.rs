use crate::{Action, ActionId, ActionPriority};
use proton_sqlite3::{rusqlite, Migration, MigratorError, SqliteConnection};
use rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{OptionalExtension, ToSql, Transaction};
use serde::de::DeserializeOwned;
use std::fmt::{Debug, Display, Formatter};
use tracing::debug;

/// Id of an action stored in the queue.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct StoredActionId(pub u64);

impl Display for StoredActionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
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
pub struct StoredAction {
    pub id: StoredActionId,
    pub action_id: ActionId,
    pub version: u32,
    pub date_time: chrono::DateTime<chrono::Utc>,
    pub priority: ActionPriority,
    data: Vec<u8>,
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

    #[cfg(test)]
    pub(crate) fn from_action_and_priority<T: Action>(
        action: &T,
        action_priority: ActionPriority,
    ) -> Result<Self, rmp_serde::encode::Error> {
        let data = rmp_serde::to_vec(action)?;

        Ok(Self {
            action_id: action.action_id().clone(),
            version: action.action_version(),
            priority: action_priority,
            data,
        })
    }
}

pub struct ActionStore<'t, 'tx: 't>(&'t mut Transaction<'tx>);

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

impl<'t, 'tx: 't> ActionStore<'t, 'tx> {
    pub fn new(tx: &'t mut Transaction<'tx>) -> Self {
        Self(tx)
    }

    pub fn init_tables(conn: &mut SqliteConnection) -> Result<(), MigratorError> {
        tracing::debug_span!("Action Table Setup").in_scope(|| {
            let migrator = proton_sqlite3::Migrator::new();
            let migrations: Vec<Box<dyn Migration>> = vec![Box::new(ActionTableMigrationV1 {})];

            let version = migrator.migrate(conn, ACTION_VERSION_TABLE_NAME, &migrations)?;
            debug!("Current version={version}");
            Ok(())
        })
    }

    pub fn get_next_action(&mut self) -> rusqlite::Result<Option<StoredAction>> {
        let query = format!(
            "SELECT {ACTION_TABLE_FIELD_ID}, {ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, \
{ACTION_TABLE_FIELD_DATE_TIME}, {ACTION_TABLE_FIELD_DATA} FROM {ACTION_TABLE_NAME} \
ORDER BY {ACTION_TABLE_FIELD_PRIORITY} ASC ,{ACTION_TABLE_FIELD_DATE_TIME} ASC"
        );

        self.0
            .query_row(&query, (), |row| {
                Ok(StoredAction {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    action_id: row.get(2)?,
                    priority: row.get(3)?,
                    date_time: row.get(4)?,
                    data: row.get(5)?,
                })
            })
            .optional()
    }
    pub fn get_stored_actions(&mut self) -> rusqlite::Result<Vec<StoredAction>> {
        let query = format!(
            "SELECT {ACTION_TABLE_FIELD_ID}, {ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, \
{ACTION_TABLE_FIELD_DATE_TIME}, {ACTION_TABLE_FIELD_DATA} FROM {ACTION_TABLE_NAME}"
        );
        let mut stmt = self.0.prepare(&query)?;
        let actions = stmt.query_map([], |row| {
            Ok(StoredAction {
                id: row.get(0)?,
                version: row.get(1)?,
                action_id: row.get(2)?,
                priority: row.get(3)?,
                date_time: row.get(4)?,
                data: row.get(5)?,
            })
        })?;

        let mut constructed = Vec::new();
        for action in actions {
            let action = action?;
            constructed.push(action)
        }

        Ok(constructed)
    }

    pub(crate) fn store_action(
        &mut self,
        action: PendingAction,
    ) -> rusqlite::Result<StoredActionId> {
        let result = self.store_actions(&[action])?;
        Ok(result[0])
    }

    pub(crate) fn store_actions(
        &mut self,
        actions: &[PendingAction],
    ) -> rusqlite::Result<Vec<StoredActionId>> {
        let query = format!("INSERT INTO {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_VERSION}, \
{ACTION_TABLE_FIELD_ACTION_ID}, {ACTION_TABLE_FIELD_PRIORITY}, {ACTION_TABLE_FIELD_DATA}) VALUES (?, ?, ?, ?)\
RETURNING {ACTION_TABLE_FIELD_ID}");
        let mut stmt = self.0.prepare(&query)?;
        let mut ids = Vec::with_capacity(actions.len());
        for action in actions {
            let id: u64 = stmt.query_row(
                (
                    action.version,
                    action.action_id.clone(),
                    action.priority,
                    &action.data,
                ),
                |r| r.get(0),
            )?;
            ids.push(StoredActionId(id));
        }
        Ok(ids)
    }

    pub fn erase_actions(&mut self, action_ids: &[StoredActionId]) -> rusqlite::Result<()> {
        let query = format!("DELETE FROM {ACTION_TABLE_NAME} WHERE {ACTION_TABLE_FIELD_ID}=?");
        let mut stmt = self.0.prepare(&query)?;

        for id in action_ids {
            stmt.execute([id.0])?;
        }

        Ok(())
    }

    pub fn tx(&mut self) -> &'_ mut Transaction<'tx> {
        self.0
    }
}

struct ActionTableMigrationV1 {}

impl Migration for ActionTableMigrationV1 {
    fn name(&self) -> &str {
        "action_table_v1"
    }

    fn migrate(&self, tx: &mut Transaction) -> rusqlite::Result<()> {
        // create actions table
        {
            let query = format!(
                "CREATE TABLE {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_ID} \
INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, {ACTION_TABLE_FIELD_ACTION_ID} BLOB NOT NULL, \
{ACTION_TABLE_FIELD_VERSION} INTEGER NOT NULL, {ACTION_TABLE_FIELD_PRIORITY} INTEGER NOT NULL, \
{ACTION_TABLE_FIELD_DATE_TIME} INTEGER DEFAULT (datetime('now')), \
{ACTION_TABLE_FIELD_DATA} BLOB NOT NULL)"
            );
            tx.execute(&query, ())?;
        }

        // Create index on Priority & Date
        let query= format!("CREATE INDEX {ACTION_TABLE_PRIORITY_INDEX_NAME} ON {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_PRIORITY})");
        tx.execute(&query, ())?;

        let query= format!("CREATE INDEX {ACTION_TABLE_DATE_TIME_INDEX_NAME} ON {ACTION_TABLE_NAME} ({ACTION_TABLE_FIELD_DATE_TIME})");
        tx.execute(&query, ())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{define_action_id, DefaultSqlConnectionProvider};
    use proton_sqlite3::InProcessTrackerService;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
    struct TestAction {
        pub value: u32,
    }

    const TEST_ACTION_VERSION: u32 = 10;
    define_action_id!(TEST_ACTION_ID, "b07e7108-6bbc-4426-9b03-67d23726bbac");
    impl Action for TestAction {
        const ID: ActionId = TEST_ACTION_ID;
        const VERSION: u32 = TEST_ACTION_VERSION;
    }

    #[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
    struct TestAction2 {
        pub value: String,
    }

    const TEST_ACTION2_VERSION: u32 = 99;
    define_action_id!(TEST_ACTION2_ID, "3e257729-7f27-42d5-b127-0d28731a69c1");
    impl Action for TestAction2 {
        const ID: ActionId = TEST_ACTION2_ID;
        const VERSION: u32 = TEST_ACTION2_VERSION;
    }

    #[test]
    fn action_insert_and_retrieval() {
        let action1 = TestAction { value: 0 };

        let action2 = TestAction2 {
            value: "hello_world!".into(),
        };

        let mut queue = new_queue();
        queue.with_store(|store| {
            let pending1 =
                PendingAction::from_action(&action1).expect("failed to create pending action");
            let pending2 =
                PendingAction::from_action(&action2).expect("failed to create pending action");
            let stored_ids = store
                .store_actions(&[pending1, pending2])
                .expect("failed to store action");

            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[0]);
                assert_eq!(stored_action.action_id, *action1.action_id());
                assert_eq!(stored_action.version, action1.action_version());
                let deserialized = stored_action
                    .deserialize::<TestAction>()
                    .expect("failed to deserialize");
                assert_eq!(deserialized, action1);
                store
                    .erase_actions(&[stored_ids[0]])
                    .expect("failed to remove stored action");
            }

            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[1]);
                assert_eq!(stored_action.action_id, *action2.action_id());
                assert_eq!(stored_action.version, action2.action_version());
                let deserialized = stored_action
                    .deserialize::<TestAction2>()
                    .expect("failed to deserialize");
                assert_eq!(deserialized, action2);
                store
                    .erase_actions(&[stored_ids[1]])
                    .expect("failed to remove stored action");
            }
        })
    }

    #[test]
    fn action_insert_and_retrieval_with_priority() {
        let action1 = TestAction { value: 0 };

        let action2 = TestAction2 {
            value: "hello_world!".into(),
        };

        let action3 = TestAction { value: 0 };

        let action4 = TestAction { value: 0 };

        let mut queue = new_queue();
        queue.with_store(|store| {
            let pending1 = PendingAction::from_action_and_priority(&action1, ActionPriority::Low)
                .expect("failed to create pending action");
            let pending2 =
                PendingAction::from_action_and_priority(&action2, ActionPriority::Highest)
                    .expect("failed to create pending action");
            let pending3 =
                PendingAction::from_action_and_priority(&action3, ActionPriority::Normal)
                    .expect("failed to create pending action");
            let pending4 = PendingAction::from_action_and_priority(&action4, ActionPriority::Low)
                .expect("failed to create pending action");
            let stored_ids = store
                .store_actions(&[pending1, pending2, pending3, pending4])
                .expect("failed to store action");

            // Actions should be consumed in the following index order: 1,2,0,3
            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[1]);
                store
                    .erase_actions(&[stored_ids[1]])
                    .expect("failed to remove stored action");
            }

            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[2]);
                store
                    .erase_actions(&[stored_ids[2]])
                    .expect("failed to remove stored action");
            }

            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[0]);
                store
                    .erase_actions(&[stored_ids[0]])
                    .expect("failed to remove stored action");
            }
            {
                let stored_action = store
                    .get_next_action()
                    .expect("failed to get next action")
                    .expect("action must be present");
                assert_eq!(stored_action.id, stored_ids[3]);
                store
                    .erase_actions(&[stored_ids[3]])
                    .expect("failed to remove stored action");
            }
        })
    }

    fn new_queue() -> crate::ActionQueue {
        let pool =
            proton_sqlite3::SqliteConnectionPool::new(proton_sqlite3::SqliteMode::InMemory, false);
        let tracker = InProcessTrackerService::new(pool).expect("failed to create tracker");
        {
            let mut conn = tracker
                .new_connection()
                .expect("failed to acquire connection");
            ActionStore::init_tables(conn.as_mut()).expect("failed to init store tables");
        }
        let factory = crate::ActionFactory::new();

        crate::ActionQueue::new(
            Box::new(DefaultSqlConnectionProvider::new(tracker)),
            Box::new(crate::AlwaysErrorSessionProvider {}),
            factory,
        )
    }
}
