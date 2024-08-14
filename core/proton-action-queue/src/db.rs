#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

use crate::action;
use crate::action::{Action, Metadata, Priority};
use chrono::{DateTime, Utc};
use indoc::indoc;
use proton_core_common::datatypes::Resources;
use proton_sqlite3::{Migration, MigratorError};
use stash::datatypes::QueryResultU64;
use stash::exports::SqliteError;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError, Tether};
use std::ops::Add;
use tracing::debug;

/// Associated action resource.
#[derive(Debug, Eq, PartialEq, Model, Clone)]
#[TableName("action_queue")]
#[ModelActions(on_load, on_save)]
pub struct StoredAction {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub action_type: String,

    /// TODO: Document this field.
    #[DbField]
    pub debug_string: Option<String>,

    /// TODO: Document this field.
    pub dependencies: Vec<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub created: DateTime<Utc>,

    /// TODO: Document this field.
    #[DbField]
    pub priority: Priority,

    /// TODO: Document this field.
    #[DbField]
    pub scheduled: DateTime<Utc>,

    /// TODO: Document this field.
    #[DbField]
    pub state: Vec<u8>,

    /// TODO: Document this field.
    pub resources: Resources,

    /// TODO: Document this field.
    #[DbField]
    pub version: u32,

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
            id: None,
            action_type: T::TYPE.to_string(),
            created: metadata.created,
            debug_string: None,
            dependencies: metadata.dependencies,
            priority: metadata.priority_override.unwrap_or(T::PRIORITY),
            resources: metadata.resources.into(),
            scheduled: delayed,
            state: serialized_state,
            version: T::VERSION,
            row_id: None,
            stash: None,
        })
    }

    pub(crate) fn short_dbg_str(&self) -> String {
        format!(
            "Action {{id={:?} type={} version={} queued={} delayed={} debug_str={} }}",
            self.id,
            self.action_type,
            self.version,
            self.created,
            self.scheduled,
            self.debug_string.as_deref().unwrap_or("")
        )
    }

    /// Return the number of pending actions in the queue.
    ///
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn pending_count(tether: &Tether) -> Result<u64, StashError> {
        let count = tether
            .query::<_, QueryResultU64>("SELECT COUNT(id) AS value FROM action_queue", vec![])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;
        Ok(count.value)
    }

    /// Check whether the action with `id` is in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn contains(tether: &Tether, id: u64) -> Result<bool, StashError> {
        let ids = tether
            .query::<_, QueryResultU64>(
                "SELECT id AS value FROM action_queue WHERE id = ?",
                params![id],
            )
            .await?;
        Ok(!ids.is_empty())
    }

    /// Extends [`Model::load()`] to load associated data.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Dependencies
        let dependencies = interface
            .query::<_, QueryResultU64>(
                "SELECT DISTINCT dependency_id AS value FROM action_queue_dependencies WHERE action_id = ?",
                params![self.id],
            )
            .await?;
        self.dependencies
            .extend(dependencies.into_iter().map(|v| v.value));

        // Resources
        self.resources = interface
            .query::<_, Resources>(
                "SELECT resource AS value FROM action_queue_resources WHERE action_id = ?",
                params![self.id],
            )
            .await?
            .pop()
            .unwrap_or_default();

        Ok(())
    }

    /// Extends [`Model::save()`] to save associated data.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Create dependencies.
        for dep in &self.dependencies {
            interface
                .execute(
                    "INSERT OR IGNORE INTO action_queue_dependencies VALUES (?,?)",
                    params![self.id, *dep],
                )
                .await?;
        }

        // Create resources
        interface
            .execute(
                "INSERT INTO action_queue_resources VALUES (?,?)",
                params![self.id, self.resources.clone()],
            )
            .await?;

        Ok(())
    }

    /// Delete action with `id` from the database.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    pub async fn delete(tether: &Tether, id: u64) -> Result<(), StashError> {
        tether
            .execute("DELETE FROM action_queue WHERE id = ?", params![id])
            .await?;
        Ok(())
    }

    /// Get all the actions which depend on the action with `id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn dependees(tether: &Tether, id: u64) -> Result<Vec<u64>, StashError> {
        let ids = tether
            .query::<_, QueryResultU64>(
                "SELECT DISTINCT action_id AS value FROM action_queue_dependencies WHERE dependency_id = ?",
                params![id],
            )
            .await?;

        Ok(ids.into_iter().map(|v| v.value).collect::<Vec<_>>())
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
        StoredAction::find_first(
            "
                WHERE
                    scheduled < ? AND (
                        SELECT COUNT(*) FROM action_queue_dependencies WHERE action_id = id
                    ) = 0
                ORDER BY
                    priority ASC, created ASC
            ",
            params![Utc::now()],
            tether,
        )
        .await
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
