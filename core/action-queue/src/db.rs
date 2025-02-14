#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

use crate::action;
use crate::action::{Action, ActionId, Metadata, Priority, Resources};
use chrono::{DateTime, Utc};
use indoc::indoc;
use proton_sqlite3::{Migration, MigratorError};
use stash::exports::SqliteError;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::ops::Add;
use tracing::{debug, error};

/// Associated action resource.
#[derive(Debug, Eq, PartialEq, Model, Clone)]
#[TableName("action_queue")]
#[ModelActions(on_load, on_save)]
pub struct StoredAction {
    /// Id assigned to any action that is stored in the queue for future
    /// execution.
    #[IdField(autoincrement)]
    pub id: Option<ActionId>,

    /// Type of the action, used to re-construct this action from a [`Factory`]
    #[DbField]
    pub action_type: String,

    /// Custom debug string that can optionally be associated with this action.
    #[DbField]
    pub debug_string: Option<String>,

    /// Other actions this action depends on.
    pub dependencies: Vec<ActionId>,

    /// Time at which this action was created.
    #[DbField]
    pub created: DateTime<Utc>,

    /// Action execution priority.
    #[DbField]
    pub priority: Priority,

    /// Time at which this action should be executed.
    #[DbField]
    pub scheduled: DateTime<Utc>,

    /// Serialized state fo the action.
    #[DbField]
    pub state: Vec<u8>,

    /// Resources associated with the action.
    pub resources: Resources,

    /// Version of the action.
    #[DbField]
    pub version: u32,

    #[DbField]
    /// Whether this action has been picked up by the queue.
    pub executing: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl StoredAction {
    /// Create a new stored action with the given `action` state and `metadata`.
    #[allow(dead_code)]
    pub(crate) fn new<T: Action>(
        action: &T,
        metadata: Metadata,
    ) -> Result<Self, rmp_serde::encode::Error> {
        let serialized_state = action::serialize(action)?;
        Ok(Self::new_impl::<T>(serialized_state, metadata))
    }

    /// Create a stored action without any state and the given `metadata`.
    pub(crate) fn without_state<T: Action>(metadata: Metadata) -> Self {
        Self::new_impl::<T>(vec![], metadata)
    }

    fn new_impl<T: Action>(state: Vec<u8>, metadata: Metadata) -> Self {
        let delayed = metadata
            .delay
            .map_or(metadata.created, |delay| metadata.created.add(delay));
        Self {
            id: None,
            action_type: T::TYPE.to_string(),
            created: metadata.created,
            debug_string: None,
            dependencies: metadata.dependencies,
            priority: metadata.priority_override.unwrap_or(T::PRIORITY),
            resources: metadata.resources,
            scheduled: delayed,
            state,
            version: T::VERSION,
            row_id: None,
            executing: false,
        }
    }

    /// Update the action state for this store action.
    ///
    /// Note this does not save to the database, use [`update_action_state()`] for that purpose.
    ///
    /// # Errors
    ///
    /// Returns error if the serialization of the action failed.
    pub(crate) fn set_action_state<T: Action>(
        &mut self,
        action: &T,
    ) -> Result<(), rmp_serde::encode::Error> {
        let serialized_state = action::serialize(action)?;
        self.state = serialized_state;
        Ok(())
    }

    /// Update the action state for this stored action.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub(crate) async fn update_action_state(&self, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            format!("UPDATE {} SET state=? WHERE id = ?", Self::table_name()),
            params![self.state.clone(), self.id.unwrap()],
        )
        .await?;
        Ok(())
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
        tether
            .query_value::<_, u64>("SELECT COUNT(id) AS value FROM action_queue", vec![])
            .await
    }

    /// Check whether the action with `id` is in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn contains(tether: &Tether, id: ActionId) -> Result<bool, StashError> {
        match tether
            .query_value::<_, ActionId>(
                "SELECT id AS value FROM action_queue WHERE id = ?",
                params![id],
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Extends [`Model::load()`] to load associated data.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_load(&mut self, tether: &Tether) -> Result<(), StashError> {
        // Dependencies
        let dependencies = tether
            .query_values::<_, ActionId>(
                "SELECT DISTINCT dependency_id AS value FROM action_queue_dependencies WHERE action_id = ?",
                params![self.id],
            )
            .await.inspect_err(|e| error!("failed to load action deps: {e:?}"))?;
        self.dependencies.extend(dependencies);

        // Resources
        match tether
            .query_value::<_, Resources>(
                "SELECT resource AS value FROM action_queue_resources WHERE action_id = ?",
                params![self.id],
            )
            .await
        {
            Ok(r) => self.resources = r,
            Err(e) => {
                error!("failed to load resources: {e:?}");
                if !matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    return Err(e);
                }
            }
        };

        Ok(())
    }

    /// Extends [`Model::save()`] to save associated data.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // Create dependencies.
        for dep in &self.dependencies {
            bond.execute(
                "INSERT OR IGNORE INTO action_queue_dependencies VALUES (?,?)",
                params![self.id, *dep],
            )
            .await?;
        }

        // Create resources
        bond.execute(
            "INSERT OR REPLACE INTO action_queue_resources VALUES (?,?)",
            params![self.id, self.resources.clone()],
        )
        .await?;

        Ok(())
    }

    /// Delete action with `id` from the database.
    ///
    /// Returns the type of the deleted action if it still exists.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    pub async fn delete(bond: &Bond<'_>, id: ActionId) -> Result<Option<String>, StashError> {
        match bond
            .query_value::<_, String>(
                "DELETE FROM action_queue WHERE id = ? RETURNING action_type AS value",
                params![id],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get all the actions which depend on the action with `id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn dependees(tehter: &Tether, id: ActionId) -> Result<Vec<ActionId>, StashError> {
        tehter
            .query_values::<_, ActionId>(
                "SELECT DISTINCT action_id AS value FROM action_queue_dependencies WHERE dependency_id = ?",
                params![id],
            )
            .await
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

    /// Create or update a stored action.
    ///
    /// An update is only applied when the existing action type matches the new action type. If
    /// the type differ a new action is stored instead.
    ///
    /// # Errors
    ///
    /// Return error if the query fails.
    pub async fn create_or_update(
        &mut self,
        existing_id: ActionId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        if let Some(existing) =
            StoredAction::find_first("WHERE id = ?", params![existing_id], bond).await?
        {
            // Only update if the action types are the same.
            // NOTE: the executing check works since we guarantee immediate locking in sqlite
            // transactions so that there is only ever one writer so this value will always be
            // up to date.
            if existing.action_type == self.action_type && !existing.executing {
                self.id = existing.id;
                self.row_id = existing.row_id;
                // failsafe, filter out any dependencies on self.
                // We also check this at submission time.
                self.dependencies.retain(|v| *v != existing_id);
            }
        }

        self.save(bond).await
    }

    /// Mark an action as executing by the queue.
    pub async fn mark_as_executing(id: ActionId, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            format!("UPDATE {} SET executing=? WHERE id =?", Self::table_name()),
            params![true, id],
        )
        .await?;
        Ok(())
    }
}

/// Create the action queue tables.
///
/// # Errors
///
/// Returns errors if the query or migration failed.
pub async fn create_tables(conn: &mut Tether) -> Result<(), MigratorError> {
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
    fn name(&self) -> &'static str {
        "action_queue_v1"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
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
                debug_string TEXT DEFAULT NULL,
                executing INTEGER NOT NULL DEFAULT 0
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
                action_id INTEGER PRIMARY KEY,
                resource BLOB NOT NULL,

                CONSTRAINT action_queue_res_action_id
                    FOREIGN KEY (action_id)
                    REFERENCES action_queue(id)
                    ON DELETE CASCADE
            )
        "};
        tx.execute(query, vec![]).await?;

        Ok(())
    }
}
