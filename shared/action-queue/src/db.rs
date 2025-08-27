#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

use crate::action::{self, ActionGroup};
use crate::action::{
    Action, ActionDependencyKey, ActionDependencyKeys, ActionId, Metadata, Priority, Resources,
    WriterGuardError,
};
use chrono::{DateTime, Utc};
use include_dir::{Dir, include_dir};
use indoc::indoc;
use proton_sqlite3::MigratorError;
use proton_sqlite3::file::embedded_migrations;
use proton_sqlite3::rusqlite::types::ValueRef;
use stash::exports::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput};
use stash::exports::{SqliteError, Value};
use stash::macros::{DbRecord, Model};
use stash::orm::{Model, ModelHooks};
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use stash::utils::{IterMapToSql, placeholders};
use std::collections::HashSet;
use std::hash::RandomState;
use std::ops::Add;
use std::time::Duration;
use tracing::error;

pub(crate) const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum DependencyType {
    /// Required dependencies result in the dependee being cancelled
    Required = 0,
    /// Optional dependencies do not result in the dependee being cancelled
    Optional = 1,
}

impl ToSql for DependencyType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for DependencyType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Required),
            1 => Ok(Self::Optional),
            v => Err(FromSqlError::OutOfRange(v.into())),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, DbRecord, Hash)]
pub struct ActionDependency {
    #[DbField]
    pub dependency_id: ActionId,
    #[DbField]
    pub dependency_type: DependencyType,
}

impl ActionDependency {
    #[must_use]
    pub fn required(action_id: ActionId) -> Self {
        Self {
            dependency_id: action_id,
            dependency_type: DependencyType::Required,
        }
    }

    #[must_use]
    pub fn optional(action_id: ActionId) -> Self {
        Self {
            dependency_id: action_id,
            dependency_type: DependencyType::Optional,
        }
    }
}

/// Associated action resource.
#[derive(Debug, Eq, PartialEq, Model, Clone)]
#[TableName("action_queue")]
#[ModelHooks]
pub struct StoredAction {
    #[IdField(autoincrement)]
    pub id: Option<ActionId>,

    #[DbField]
    pub action_type: String,

    #[DbField]
    pub debug_string: Option<String>,

    pub dependencies: Vec<ActionDependency>,

    #[DbField]
    pub created: DateTime<Utc>,

    #[DbField]
    pub priority: Priority,

    #[DbField]
    pub scheduled: DateTime<Utc>,

    #[DbField]
    pub state: Vec<u8>,

    #[DbField]
    pub action_group: String,

    pub resources: Resources,

    #[DbField]
    pub version: u32,

    // Note this field is only used for storage into the db.
    pub dependency_keys: ActionDependencyKeys,
}

impl StoredAction {
    /// Create a new stored action with the given `action` state and `metadata`.
    #[allow(dead_code)]
    pub(crate) fn new<T: Action>(
        action: &T,
        metadata: Metadata,
    ) -> Result<Self, rmp_serde::encode::Error> {
        let serialized_state = action::serialize(action)?;
        Ok(Self::new_impl::<T>(
            serialized_state,
            action.dependency_keys(),
            metadata,
        ))
    }

    #[must_use]
    /// Create a stored action without any state and the given `metadata`.
    pub fn without_state<T: Action>(
        dependency_keys: ActionDependencyKeys,
        metadata: Metadata,
    ) -> Self {
        Self::new_impl::<T>(vec![], dependency_keys, metadata)
    }

    fn new_impl<T: Action>(
        state: Vec<u8>,
        dependency_keys: ActionDependencyKeys,
        metadata: Metadata,
    ) -> Self {
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
            action_group: metadata.group_override.unwrap_or(T::GROUP).to_string(),
            dependency_keys,
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
            "Action {{ version={} queued={} delayed={} debug_str={} }}",
            self.version,
            self.created,
            self.scheduled,
            self.debug_string.as_deref().unwrap_or(""),
        )
    }

    /// Return the number of pending actions in the queue.
    ///
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn pending_count(tether: &Tether) -> Result<u64, StashError> {
        Self::count("", vec![], tether).await
    }

    /// Return the number of pending actions in the queue for a given action type.
    ///
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    ///
    pub async fn type_count<T: Action>(tether: &Tether) -> Result<u64, StashError> {
        Self::count("where action_type = ?", params![T::TYPE.as_ref()], tether).await
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

    /// Delete all actions from the database within specified action group.
    ///
    /// # Warning
    ///
    /// This operation does not operate within execution guards. It is intended to be used
    /// before queue executor is resumed (during app initialization). Use with caution.
    pub async fn delete_all_in_group(
        bond: &Bond<'_>,
        group: ActionGroup,
    ) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM action_queue WHERE action_group = ?",
            params![group.as_ref().to_owned()],
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
    pub async fn all_dependees(
        tether: &Tether,
        id: ActionId,
    ) -> Result<Vec<ActionDependency>, StashError> {
        tether
            .query::<_, ActionDependency>(
                "SELECT * FROM action_queue_dependencies WHERE dependency_id = ?",
                params![id],
            )
            .await
    }

    pub async fn all_dependencies(
        tether: &Tether,
        id: ActionId,
    ) -> Result<Vec<ActionDependency>, StashError> {
        tether
            .query::<_, ActionDependency>(
                "SELECT * FROM action_queue_dependencies WHERE action_id = ?",
                params![id],
            )
            .await
    }

    /// Get all the actions which depend on the action with `id` with a given dependency type.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn dependees_of_type(
        tether: &Tether,
        id: ActionId,
        dependency_type: DependencyType,
    ) -> Result<Vec<ActionId>, StashError> {
        tether
            .query_values::<_, ActionId>(
                "SELECT DISTINCT action_id AS value FROM action_queue_dependencies WHERE dependency_id = ? AND dependency_type =?",
                params![id, dependency_type],
            )
            .await
    }

    /// Get the next action to be executed in the given `action_group`.
    ///
    /// This takes into account dependencies, priority and execution delays. If `None` is returned
    /// from this function there are no actions that can be executed at this point.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub(crate) async fn next(
        action_group: &str,
        tether: &Tether,
    ) -> Result<Option<StoredAction>, StashError> {
        Self::next_with_timeout(action_group, DEFAULT_LOCK_TIMEOUT, tether).await
    }

    /// Get the next action to be executed in the given `action_group`.
    ///
    /// This takes into account dependencies, priority and execution delays. If `None` is returned
    /// from this function there are no actions that can be executed at this point.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    async fn next_with_timeout(
        action_group: &str,
        timeout: Duration,
        tether: &Tether,
    ) -> Result<Option<StoredAction>, StashError> {
        let now = Utc::now();
        StoredAction::find_first(
            "
                LEFT JOIN action_queue_lock ON action_queue.id = action_queue_lock.action_id
                WHERE
                    action_group = ?3 AND
                    (
                        action_queue_lock.action_id IS NULL OR
                        unixepoch(datetime(?1)) - unixepoch(datetime(action_queue_lock.acquired_at)) >= ?2
                    ) AND
                    scheduled < ?1 AND (
                        SELECT COUNT(*) FROM action_queue_dependencies WHERE action_id = id
                    ) = 0
                ORDER BY
                    priority ASC, created ASC
            ",
            params![now, timeout.as_secs(), action_group.to_owned()],
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
            let is_executing = ExecutionGuard::has_executor(existing_id, bond).await?;
            if existing.action_type == self.action_type && !is_executing {
                self.id = existing.id;
                // failsafe, filter out any dependencies on self.
                // We also check this at submission time.
                self.dependencies.retain(|v| v.dependency_id != existing_id);
                // Remove any dependency key associated with the old action to prevent cyclic
                // references
                ActionDependencyKeysTable::delete_for_action_id(existing_id, bond).await?;
            }
        }

        self.save(bond).await
    }

    /// Pop an action from the queue for a given `action_group`.
    ///
    /// This takes into account dependencies, priority and execution delays. If `None` is returned
    /// from this function there are no actions that can be executed at this point.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed or the executor could not be retrieved.
    pub async fn pop(
        executor_id: String,
        action_group: &str,
        tether: &mut Tether,
    ) -> Result<Option<(ExecutionGuard, StoredAction)>, StashError> {
        tether
            .tx(async |tx| {
                ExecutionGuard::clear_slate_state(executor_id.clone(), tx).await?;
                let next_action = Self::next(action_group, tx).await?;

                let Some(next_action) = next_action else {
                    return Ok(None);
                };

                let guard =
                    ExecutionGuard::acquire(next_action.id.unwrap(), executor_id, tx).await?;
                Ok(Some((guard, next_action)))
            })
            .await
    }
}

impl ModelHooks for StoredAction {
    async fn after_load(&mut self, tether: &Tether) -> Result<(), StashError> {
        // Dependencies
        let dependencies = Self::all_dependencies(tether, self.id())
            .await
            .inspect_err(|e| error!("failed to load action deps: {e:?}"))?;
        self.dependencies.extend(dependencies);

        // Resources
        match tether
            .query_value_opt::<Resources>(
                "SELECT resource AS value FROM action_queue_resources WHERE action_id = ?",
                params![self.id],
            )
            .await?
        {
            Some(r) => self.resources = r,
            None => {
                error!("failed to load resources");
            }
        }

        Ok(())
    }

    async fn after_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // Resolve dependencies from keys
        let direct_dependencies = ActionDependencyKeysTable::resolve_dependency_keys(
            self.dependency_keys.required.clone(),
            bond,
        )
        .await?
        .into_iter()
        .map(ActionDependency::required)
        .collect::<Vec<_>>();
        let sequential_dependencies = ActionDependencyKeysTable::resolve_dependency_keys(
            self.dependency_keys.optional.clone(),
            bond,
        )
        .await?
        .into_iter()
        .map(ActionDependency::optional)
        .collect::<Vec<_>>();

        let dependency_set: HashSet<ActionDependency> = self
            .dependencies
            .iter()
            .chain(sequential_dependencies.iter())
            .chain(direct_dependencies.iter())
            .cloned()
            .collect();

        // Create dependencies.
        if !dependency_set.is_empty() {
            // Insert or ignore doesn't take into account that the foreign key does not exist.
            // This is an SQLite limitation. So we need to manually check this before inserts.
            #[allow(trivial_casts)]
            let parameters = dependency_set
                .iter()
                .map(|dep| dep.dependency_id)
                .bridge_sql();
            let placeholders = placeholders(&parameters);
            let existing_action_ids: HashSet<ActionId, RandomState> = HashSet::from_iter(
                bond.query_values::<_, ActionId>(
                    format!(
                        "SELECT id AS value FROM {} WHERE id IN ({placeholders})",
                        Self::table_name()
                    ),
                    parameters,
                )
                .await?,
            );

            for dep in dependency_set {
                if existing_action_ids.contains(&dep.dependency_id) {
                    bond.execute(
                        indoc! {
                            "INSERT INTO action_queue_dependencies (action_id, dependency_id, dependency_type)
                             VALUES (?,?,?)
                             ON CONFLICT DO UPDATE SET dependency_type = excluded.dependency_type
                            "
                        },
                        params![self.id, dep.dependency_id, dep.dependency_type],
                    )
                    .await?;
                }
            }
        }

        // Create resources
        bond.execute(
            "INSERT OR REPLACE INTO action_queue_resources VALUES (?,?)",
            params![self.id, self.resources.clone()],
        )
        .await?;

        // Update direct dependency keys
        ActionDependencyKeysTable::store_dependency_keys(
            self.dependency_keys
                .required
                .iter()
                .chain(self.dependency_keys.record.iter())
                .cloned()
                .collect(),
            self.id(),
            bond,
        )
        .await?;

        Ok(())
    }
}

/// An execution guard for Queue Executors to prevent an action to be executed more than
/// once at the same time.
///
/// Each time an action is meant to be executed the guard will be acquired. The guard
/// remains valid for certain amount of time before expiring. When a guard expires, the next
/// attempt to acquire it will bump the permit id.
///
/// The permit id is checked when we try to create a transaction. If for some reason another executor
/// has started working on the action, the permit id will no longer match and we abort.
///
/// This type is not a [`Model`] to avoid accidental changes to the data.
pub struct ExecutionGuard {
    action_id: ActionId,
    permit_id: usize,
}

impl ExecutionGuard {
    /// Check whether the action with `action_id` is being executed.
    pub async fn has_executor(action_id: ActionId, bond: &Bond<'_>) -> Result<bool, StashError> {
        // While this function could be written to accept a Tether instead, it would bypass
        // the exclusive writer access, which is required for this to work.
        let has_executor = match bond
            .query_value::<_, bool>(
                "SELECT executor_id IS NOT NULL AS value FROM action_queue_lock WHERE action_id = ?",
                params![action_id],
            )
            .await
        {
            Ok(has_executor) => has_executor,
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => false,
            Err(e) => return Err(e),
        };
        Ok(has_executor)
    }

    /// Acquire the execution rights for the action with `action_id`.
    ///
    /// `executor_id` is a debug string that is recorded and should be unique per executor.
    ///
    /// # Remarks
    ///
    /// This method does not check if we can legally acquire the execution lock.
    /// [`StoredAction::next()`] performs all the checks and returns the next action that
    /// can be acquired.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn acquire(
        action_id: ActionId,
        executor_id: impl Into<String>,
        bond: &Bond<'_>,
    ) -> Result<Self, StashError> {
        Self::acquire_with_timestamp(action_id, executor_id, Utc::now(), bond).await
    }

    /// Same as [`acquire`] but allows one to specify the [`timestamp`] of acquisition.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn acquire_with_timestamp(
        action_id: ActionId,
        executor_id: impl Into<String>,
        timestamp: DateTime<Utc>,
        bond: &Bond<'_>,
    ) -> Result<Self, StashError> {
        let executor_id = executor_id.into();
        let permit_id = bond
            .query_value::<_, usize>(
                indoc! {"
            INSERT INTO action_queue_lock (action_id, executor_id, acquired_at, permit_id)
            VALUES (?1,?2,?3, 1)
            ON CONFLICT (action_id) DO UPDATE SET
                executor_id = ?2,
                permit_id=permit_id +1,
                acquired_at = ?3
            RETURNING permit_id AS value
       "},
                params![action_id, executor_id, timestamp],
            )
            .await?;

        Ok(Self {
            action_id,
            permit_id,
        })
    }

    /// Clean any leftover stale locks. These can occur if the execution of background task
    /// is aborted or if for some reason we never managed to properly release our previous lock.
    pub(crate) async fn clear_slate_state(
        executor_id: String,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM action_queue_lock WHERE executor_id= ?",
            params![executor_id],
        )
        .await?;
        Ok(())
    }

    /// Release the current access privileges.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    pub async fn release(self, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            indoc! {"
            UPDATE action_queue_lock SET
                executor_id = NULL,
                acquired_at = 0
            WHERE action_id = ? AND permit_id = ?
       "},
            params![self.action_id, self.permit_id],
        )
        .await?;
        Ok(())
    }

    /// Create a new transaction.
    ///
    /// This internally checks whether the permit id still matches what we expect the value to be.
    /// If this is not the case, this lock expired and we should not write to the database.
    ///
    /// Every time we are able to write with a valid permit, we also update
    /// the timestamp. This allows for some longer running tasks to extend their lifetime a bit
    /// and prevent unnecessary re-runs.
    ///
    /// To prevent
    ///
    /// # Errors
    ///
    /// Returns [`StashError`] if the transaction failed to acquire and [`WriterGuardError::Expired`]
    /// if this execution lock has expired.
    pub async fn tx<F, T, E>(&self, tether: &mut Tether, closure: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, E>,
        E: From<WriterGuardError> + From<StashError>,
    {
        tether.tx(async |tx| {
                let changed = tx
                    .execute(
                        "UPDATE action_queue_lock SET acquired_at=? WHERE action_id=? AND permit_id =?",
                        params![Utc::now(), self.action_id, self.permit_id],
                    )
                    .await?;
                if changed == 0 {
                    return Err(WriterGuardError::Expired.into());
                }
                closure(tx).await
            }).await
    }

    /// Same as [`transaction`], but releases the guard when finished.
    pub(crate) async fn tx_and_release<F, T>(
        self,
        tether: &mut Tether,
        closure: F,
    ) -> Result<T, WriterGuardError>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, StashError>,
    {
        tether
            .tx(async |tx| {
                let changed = tx
                .execute(
                    "UPDATE action_queue_lock SET acquired_at=? WHERE action_id=? AND permit_id =?",
                    params![Utc::now(), self.action_id, self.permit_id],
                )
                .await?;
                if changed == 0 {
                    return Err(WriterGuardError::Expired);
                }
                let r = closure(tx).await;
                self.release(tx).await?;
                Ok(r?)
            })
            .await
    }
}

#[tracing::instrument(name = "Action Table Setup", skip(conn))]
pub async fn migrate(conn: &mut Tether) -> Result<(), MigratorError> {
    const TABLE: &str = "action_queue_version";
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/db/migrations");

    proton_sqlite3::Migrator::new(TABLE, embedded_migrations(&MIGRATIONS))
        .migrate(conn)
        .await?;

    Ok(())
}

pub struct ActionDependencyKeysTable {}

const KEY_DEPENDENCIES_TABLE_NAME: &str = "action_queue_key_deps_v2";
impl ActionDependencyKeysTable {
    pub async fn resolve_dependency_keys(
        keys: Vec<ActionDependencyKey>,
        tether: &Tether,
    ) -> Result<Vec<ActionId>, StashError> {
        let parameters = keys.into_iter().bridge_sql();
        if parameters.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = placeholders(&parameters);
        tether
            .query_values::<_, ActionId>(
                format!(
                    "SELECT DISTINCT action_id AS value FROM {KEY_DEPENDENCIES_TABLE_NAME} WHERE key_id IN ({placeholders})",
                ),
                parameters,
            )
            .await
    }

    pub async fn store_dependency_keys(
        keys: Vec<ActionDependencyKey>,
        action_id: ActionId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for key in keys {
            bond.execute(
                format!(
                    "INSERT INTO {KEY_DEPENDENCIES_TABLE_NAME} (key_id, action_id) VALUES (?,?)",
                ),
                params![key, action_id],
            )
            .await?;
        }

        Ok(())
    }

    pub async fn delete_for_action_id(
        action_id: ActionId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            format!("DELETE FROM {KEY_DEPENDENCIES_TABLE_NAME} WHERE action_id = ?"),
            params![action_id],
        )
        .await?;
        Ok(())
    }
}
