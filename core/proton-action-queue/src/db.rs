#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

use crate::action;
use crate::action::{Action, Id, Metadata, Priority};
use indoc::indoc;
use proton_sqlite3::{Migration, MigratorError};
use rusqlite::ToSql;
use stash::macros::DbRecord;
use stash::orm::{DbRecord, DbRecords};
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use std::ops::Add;
use tracing::debug;

/// Associated action resource.
#[derive(Debug, Eq, PartialEq, DbRecord, Clone)]
pub struct StoredAction {
    #[DbField]
    pub id: Id,
    #[DbField]
    pub action_type: String,
    #[DbField]
    pub version: u32,
    #[DbField]
    pub created: chrono::DateTime<chrono::Utc>,
    #[DbField]
    pub scheduled: chrono::DateTime<chrono::Utc>,
    #[DbField]
    pub priority: Priority,
    #[DbField]
    pub state: Vec<u8>,
    pub dependencies: Vec<Id>,
    #[DbField]
    pub debug_string: Option<String>,
    pub resources: Vec<Vec<u8>>,
}
impl StoredAction {
    pub(crate) fn new<T: Action>(
        action: &T,
        metadata: Metadata,
    ) -> Result<Self, rmp_serde::encode::Error> {
        let serialized_state = action::serialize(action)?;
        let delayed = metadata
            .delay
            .map_or(metadata.created, |delay| metadata.created.add(delay));
        Ok(Self {
            id: Id(0),
            action_type: T::TYPE.to_string(),
            version: T::VERSION,
            created: metadata.created,
            scheduled: delayed,
            priority: metadata.priority_override.unwrap_or(T::PRIORITY),
            state: serialized_state,
            dependencies: metadata.dependencies,
            debug_string: None,
            resources: metadata.resources,
        })
    }
    pub(crate) fn short_dbg_str(&self) -> String {
        format!(
            "Action {{id={} type={} version={} queued={} delayed={} debug_str={} }}",
            self.id,
            self.action_type,
            self.version,
            self.created,
            self.scheduled,
            self.debug_string.as_deref().unwrap_or("")
        )
    }
}

impl StoredAction {
    /// Get a stored action by `id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn with_id(tether: &Tether, id: Id) -> Result<Option<StoredAction>, StashError> {
        load_action(tether, id).await
    }

    /// Return the number of pending actions in the queue.
    ///
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn pending_count(tether: &Tether) -> Result<usize, StashError> {
        #[derive(Debug, DbRecord, Eq, PartialEq, Clone)]
        struct Record {
            #[DbField]
            count: usize,
        }

        let count = tether
            .query_row::<_, Record>("SELECT COUNT(id) as count FROM action_queue", vec![])
            .await?;
        Ok(count.count)
    }

    /// Check whether the action with `id` is in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn contains(tether: &Tether, id: Id) -> Result<bool, StashError> {
        let ids = tether
            .query::<_, IdRecord>("SELECT id FROM action_queue WHERE id=?", params![id])
            .await?;
        Ok(!ids.is_empty())
    }

    /// Store the `action` in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn store(tether: &Tether, action: StoredAction) -> Result<Id, StashError> {
        let id = tether
            .query_row::<_, IdRecord>(
                indoc! {"
            INSERT INTO action_queue (
                `action_type`,
                version,
                priority,
                created,
                scheduled,
                state,
                debug_string
            ) VALUES (?,?,?,?,?,?,?)
            RETURNING id
        "},
                params![
                    action.action_type,
                    action.version,
                    action.priority,
                    action.created,
                    action.scheduled,
                    action.state,
                    action.debug_string
                ],
            )
            .await?;

        let id = id.id;

        // Create dependencies.
        for dep in action.dependencies {
            tether
                .execute(
                    "INSERT OR IGNORE INTO action_queue_dependencies VALUES (?,?)",
                    params![id, dep],
                )
                .await?;
        }

        // Create resources
        for resource in action.resources {
            tether
                .execute(
                    "INSERT INTO action_queue_resources VALUES (?,?)",
                    params![id, resource],
                )
                .await?;
        }

        Ok(id)
    }

    /// Delete action with `id` from the database.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    pub async fn delete(tether: &Tether, id: Id) -> Result<(), StashError> {
        tether
            .execute("DELETE FROM action_queue WHERE id=?", params![id])
            .await?;
        Ok(())
    }

    /// Get all the actions which depend on the action with `id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn dependees(tether: &Tether, id: Id) -> Result<Vec<Id>, StashError> {
        let ids = tether
            .query::<_, IdRecord>(
                "SELECT DISTINCT action_id as id FROM action_queue_dependencies WHERE dependency_id=?",
                params![id],
            )
            .await?;

        Ok(ids.into_iter().map(|v| v.id).collect::<Vec<_>>())
    }

    /// Get the next action to be executed.
    ///
    /// This takes into account dependencies, priority and execution delays. If `None` is returned
    /// from this function there are no actions that can be executed at this point.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn next(tether: &Tether) -> Result<Option<StoredAction>, StashError> {
        let dt_now = chrono::Utc::now();
        let Some(mut action) = tether
            .query_row(
                indoc::formatdoc! {"
                    {prelude}
                    WHERE scheduled < ? AND (
                        SELECT COUNT(*) FROM action_queue_dependencies WHERE action_id=id
                    )=0
                    ORDER BY priority ASC, created ASC LIMIT 1
                ", prelude=SELECT_ACTION_PRELUDE},
                params![dt_now],
            )
            .await
            .optional()?
        else {
            return Ok(None);
        };

        load_action_dependencies_and_resources(tether, &mut action).await?;

        Ok(Some(action))
    }
}

#[allow(async_fn_in_trait)]
pub trait ActionQueueExtension {
    /// This extension expects at least one row to be returned. If no rows are returned
    /// it is considered an error.
    ///
    /// Combine this with [`OptionalExtension`] to allow missing values.
    async fn query_row<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<T, StashError>
    where
        Q: Into<String> + Send,
        T: DbRecord + Send + 'static,
        DbRecords: FromIterator<Box<T>>;
}
impl ActionQueueExtension for Tether {
    async fn query_row<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<T, StashError>
    where
        Q: Into<String> + Send,
        T: DbRecord + Send + 'static,
        DbRecords: FromIterator<Box<T>>,
    {
        let mut v = self.query::<Q, T>(query, params).await?;
        if v.is_empty() {
            return Err(StashError::ExecutionError(
                rusqlite::Error::QueryReturnedNoRows,
            ));
        };

        Ok(v.swap_remove(0))
    }
}

pub trait OptionalExtension<T> {
    /// Optional conversion for stash errors.
    ///
    /// # Errors
    ///
    /// If the error equals [`rusqlite::Error::QueryReturnedNoRows`], this method should return
    /// `Ok(None)`. Otherwise, the original error will be passed along.
    fn optional(self) -> Result<Option<T>, StashError>;
}

impl<T> OptionalExtension<T> for Result<T, StashError> {
    fn optional(self) -> Result<Option<T>, StashError> {
        match self {
            Ok(t) => Ok(Some(t)),
            Err(StashError::ExecutionError(rusqlite::Error::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Create the action queue tables.
///
/// # Errors
///
/// Returns errors if the query or migration failed.
pub async fn create_tables(conn: &Stash) -> Result<(), MigratorError> {
    let span = tracing::debug_span!("Action Table Setup");
    let _enter = span.enter();
    let migrator = proton_sqlite3::Migrator::new();
    let migrations = vec![MigrationV1 {}];

    let version = migrator
        .migrate(conn, ACTION_VERSION_TABLE_NAME, &migrations)
        .await?;
    debug!("Current version={version}");
    Ok(())
}

const SELECT_ACTION_PRELUDE: &str = indoc! {"
                SELECT
                    id,
                    `action_type`,
                    version,
                    created,
                    scheduled,
                    priority,
                    state,
                    debug_string
                FROM action_queue
           "
};
async fn load_action(conn: &Tether, id: Id) -> Result<Option<StoredAction>, StashError> {
    let Some(mut action) = conn
        .query_row::<_, StoredAction>(
            &indoc::formatdoc! {"{prelude} WHERE id=? LIMIT 1", prelude=SELECT_ACTION_PRELUDE},
            params![id],
        )
        .await
        .optional()?
    else {
        return Ok(None);
    };

    load_action_dependencies_and_resources(conn, &mut action).await?;

    Ok(Some(action))
}

#[derive(Debug, DbRecord, Eq, PartialEq, Clone)]
struct IdRecord {
    #[DbField]
    pub id: Id,
}

async fn load_action_dependencies_and_resources(
    conn: &Tether,
    action: &mut StoredAction,
) -> Result<(), StashError> {
    #[derive(Debug, DbRecord, Eq, PartialEq, Clone)]
    struct Res {
        #[DbField]
        pub resource: Vec<u8>,
    }
    // Dependencies
    {
        let results = conn
            .query::<_, IdRecord>(
                "SELECT DISTINCT dependency_id as id FROM action_queue_dependencies WHERE action_id=?",
                params![action.id],
            )
            .await?;
        action
            .dependencies
            .extend(results.into_iter().map(|v| v.id));
    }

    // Resources
    {
        let results = conn
            .query::<_, Res>(
                "SELECT resource FROM action_queue_resources WHERE action_id=?",
                params!(action.id),
            )
            .await?;

        action
            .resources
            .extend(results.into_iter().map(|v| v.resource));
    }

    Ok(())
}

const ACTION_VERSION_TABLE_NAME: &str = "action_queue_version";
struct MigrationV1 {}

impl Migration for MigrationV1 {
    fn name(&self) -> &str {
        "action_queue_v1"
    }

    async fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
        // create actions table
        let query = indoc! {"
            CREATE TABLE action_queue (
                id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                `action_type` TEXT NOT NULL,
                version INTEGER NOT NULL,
                priority INTEGER NOT NULL,
                created INTEGER DEFAULT (datetime('now')),
                scheduled INTEGER DEFAULT (datetime('now')),
                state BLOB NOT NULL,
                debug_string TEXT DEFAULT NULL
            )
        "};

        tx.execute(query, vec![]).await?;

        // Create index on Priority & Date
        let query = "CREATE INDEX action_queue_idx_prio ON action_queue (priority)";
        tx.execute(query, vec![]).await?;

        let query = "CREATE INDEX action_queue_idx_date ON action_queue (created)";
        tx.execute(query, vec![]).await?;

        let query = "CREATE INDEX action_queue_idx_delay ON action_queue (scheduled)";
        tx.execute(query, vec![]).await?;

        // Create dependencies table
        let query = indoc! {"
            CREATE TABLE action_queue_dependencies (
                action_id INTEGER NOT NULL,
                dependency_id INTEGER NOT NULL,
                PRIMARY KEY(action_id, dependency_id),

                CONSTRAINT action_queue_dep_action_id
                    FOREIGN KEY (action_id)
                    REFERENCES action_queue(id)
                    ON DELETE CASCADE,

                CONSTRAINT action_queue_dep_dep_id
                    FOREIGN KEY (dependency_id)
                    REFERENCES action_queue(id)
                    ON DELETE CASCADE
            )
        "};
        tx.execute(query, vec![]).await?;

        // Create resource tables
        let query = indoc! {"
            CREATE TABLE action_queue_resources (
                action_id INTEGER NOT NULL,
                resource BLOB NOT NULL,

                CONSTRAINT action_queue_res_action_id
                    FOREIGN KEY (action_id)
                    REFERENCES action_queue(id)
                    ON DELETE CASCADE
            )
        "};
        tx.execute(query, vec![]).await?;

        let query = "CREATE INDEX action_queue_resources_idx ON action_queue_resources (action_id)";
        tx.execute(query, vec![]).await?;

        Ok(())
    }
}
