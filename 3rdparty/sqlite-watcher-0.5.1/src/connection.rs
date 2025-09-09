use crate::statement::{
    BatchQuery, Sealed, SqlExecuteStatement, SqlTransactionStatement, Statement, StatementWithInput,
};
use crate::watcher::{ObservedTableOp, Watcher};
use fixedbitset::FixedBitSet;
use std::error::Error;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tracing::{debug, trace, warn};

#[cfg(feature = "rusqlite")]
pub mod rusqlite;

#[cfg(feature = "sqlx")]
pub mod sqlx;

/// Defines an implementation capable of executing SQL statement on a sqlite connection.
///
/// This is required so we can set up the temporary triggers and tables required to
/// track changes.
pub trait SqlExecutor {
    type Error: Error;
    /// This method will execute a query which returns 0 or N rows with one column of type `u32`.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_query_values(&self, query: &str) -> Result<Vec<u32>, Self::Error>;

    /// Execute an sql statement which does not return any rows.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_execute(&self, query: &str) -> Result<(), Self::Error>;
}

/// Similar to [`SqlExecutor`], but for implementations that require mutable access to
/// the connection to work.
pub trait SqlExecutorMut {
    type Error: Error;
    /// This method will execute a query which returns 0 or N rows with one column of type `u32`.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_query_values(&mut self, query: &str) -> Result<Vec<u32>, Self::Error>;

    /// Execute an sql statement which does not return any rows.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_execute(&mut self, query: &str) -> Result<(), Self::Error>;
}

// Automatically derive SqlExecutorMut for any implementation of SqlExecutor.
impl<T: SqlExecutor> SqlExecutorMut for T {
    type Error = T::Error;

    fn sql_query_values(&mut self, query: &str) -> Result<Vec<u32>, Self::Error> {
        SqlExecutor::sql_query_values(self, query)
    }

    fn sql_execute(&mut self, query: &str) -> Result<(), Self::Error> {
        SqlExecutor::sql_execute(self, query)
    }
}

/// Defines an implementation capable of executing SQL statement on a sqlite connection.
///
/// This is required so we can set up the temporary triggers and tables required to
/// track changes.
pub trait SqlExecutorAsync: Send {
    type Error: Error + Send;
    /// This method will execute a query which returns 0 or N rows with one column of type `u32`.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_query_values(
        &mut self,
        query: &str,
    ) -> impl Future<Output = Result<Vec<u32>, Self::Error>> + Send;

    /// Execute an sql statement which does not return any rows.
    ///
    /// # Errors
    ///
    /// Should return error if the query failed.
    fn sql_execute(&mut self, query: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Building block to provide tracking capabilities to any type of sqlite connection which
/// implements the [`SqlExecutor`] trait.
///
/// # Initialization
///
/// It's recommended to call [`State::set_pragmas()`] to enable in memory temporary tables and recursive
/// triggers. If your connection already has this set up, this can be skipped.
///
/// Next you need to create the infrastructure to track changes. This can be accomplished with
/// [`State::start_tracking()`].
///
/// # Tracking changes
///
/// To make sure we only track required tables always call [`State::sync_tables()`] before a query/statement
/// or a transaction.
///
/// When the query/statement or transaction are completed, call [`State::publish_changes()`] to check
/// which tables have been modified and send this information to the watcher.
///
/// # Disable Tracking
///
/// If you wish to remove all the tracking infrastructure from a connection on which
/// [`State::start_tracking()`] was called, then call [`State::stop_tracking()`].
///
/// # See Also
///
/// The [`Connection`] type provided by this crate provides an example integration implementation.
#[derive(Debug, Default)]
pub struct State {
    tracked_tables: FixedBitSet,
    last_sync_version: u64,
}

impl State {
    /// Enable required pragmas for execution.
    #[must_use]
    pub fn set_pragmas() -> impl Statement {
        SqlExecuteStatement::new("PRAGMA temp_store = MEMORY")
            .then(SqlExecuteStatement::new("PRAGMA recursive_triggers='ON'"))
    }

    /// Prepare the `connection` for tracking.
    ///
    /// This will create the temporary table used to track change.
    #[must_use]
    #[tracing::instrument(level = tracing::Level::DEBUG)]
    pub fn start_tracking() -> impl Statement {
        // create tracking table and cleanup previous data if re-used from a connection pool.
        SqlTransactionStatement::temporary(
            SqlExecuteStatement::new(create_tracking_table_query())
                .then(SqlExecuteStatement::new(empty_tracking_table_query())),
        )
        .spanned_in_current()
    }

    /// Remove all triggers and the tracking table from `connection`.
    //
    /// # Errors
    ///
    /// Returns error if the initialization failed.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn stop_tracking(&self, watcher: &Watcher) -> impl Statement {
        let tables = watcher.observed_tables();
        SqlTransactionStatement::temporary(
            BatchQuery::new(
                tables
                    .into_iter()
                    .enumerate()
                    .flat_map(|(id, table_name)| drop_triggers(&table_name, id)),
            )
            .then(SqlExecuteStatement::new(drop_tracking_table_query())),
        )
        .spanned_in_current()
    }

    /// Create a new instance without initializing any connection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracked_tables: FixedBitSet::new(),
            last_sync_version: 0,
        }
    }

    /// Synchronize the table list from the watcher.
    ///
    /// This method will create new triggers for tables that are not being watched over this
    /// connection and remove triggers for tables that are no longer observed by the watcher.
    ///
    /// # Errors
    ///
    /// Returns error if creation or removal of triggers failed.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self, watcher))]
    pub fn sync_tables(&mut self, watcher: &Watcher) -> Option<impl Statement + '_> {
        let new_version = self.should_sync(watcher)?;

        debug!("Syncing tables from observer");
        let Some((new_tracker_state, tracker_changes)) = self.calculate_sync_changes(watcher)
        else {
            debug!("No changes");
            return None;
        };

        let mut queries = BatchQuery::new([]);

        if self.tracked_tables.is_empty() {
            // It is possible on certain circumstances that if a connection can have leftover
            // tracking data that is not cleared. To make sure this is reset, we force empty
            // the table if we detect that we are not watching any tables at the moment.
            queries.push(SqlExecuteStatement::new(empty_tracking_table_query()));
        }
        for change in tracker_changes {
            match change {
                ObservedTableOp::Add(table_name, id) => {
                    debug!("Add watcher for table {table_name} id={id}");
                    queries.extend(create_triggers(&table_name, id));
                }
                ObservedTableOp::Remove(table_name, id) => {
                    debug!("Remove watcher for table {table_name}");
                    queries.extend(drop_triggers(&table_name, id));
                }
            }
        }

        let tx = SqlTransactionStatement::temporary(queries);
        Some(
            tx.then(ConcludeStateChangeStatement {
                state: self,
                tracked_tables: new_tracker_state,
                new_version,
            })
            .spanned_in_current(),
        )
    }

    /// Check the tracking table and report finding to the [Watcher].
    ///
    /// The table where the changes are tracked is read and reset. Any
    /// table that has been modified will be communicated to the [Watcher], which in turn
    /// will notify the respective [TableObserver].
    ///
    /// # Errors
    ///
    /// Returns error if we failed to read from the temporary tables.
    ///
    /// [Watcher]: `crate::watcher::Watcher`
    /// [TableObserver]: `crate::watcher::TableObserver`
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self, watcher))]
    pub fn publish_changes(&self, watcher: &Watcher) -> impl Statement {
        SqlReadTableIdsStatement
            .pipe(CalculateWatcherUpdatesStatement { state: self })
            .pipe(MaybeResetResultsQuery)
            .pipe(PublishWatcherChangesStatement(watcher))
            .spanned_in_current()
    }

    fn prepare_watcher_changes(&self, modified_table_ids: Vec<u32>) -> FixedBitSet {
        trace!("Preparing watcher changes");
        let mut result = FixedBitSet::with_capacity(self.tracked_tables.len());
        for id in modified_table_ids {
            let id = id as usize;
            debug!("Table {} has been modified", id);
            if id >= result.len() {
                warn!(
                    "Received update for table {id}, but only tracking {} tables",
                    self.tracked_tables.len(),
                );
                // We need to grow on the index + 1.
                result.grow(id + 1);
            }
            result.set(id, true);
        }

        result
    }

    fn should_sync(&self, watcher: &Watcher) -> Option<u64> {
        let service_version = watcher.tables_version();
        if service_version == self.last_sync_version {
            None
        } else {
            Some(service_version)
        }
    }

    /// Determine which tables should start and/or stop being watched.
    fn calculate_sync_changes(
        &self,
        watcher: &Watcher,
    ) -> Option<(FixedBitSet, Vec<ObservedTableOp>)> {
        trace!("Calculating sync changes");
        let (new_tracker_state, tracker_changes) =
            watcher.calculate_sync_changes(&self.tracked_tables);

        if tracker_changes.is_empty() {
            return None;
        }

        Some((new_tracker_state, tracker_changes))
    }

    /// Once we are satisfied with the changes, apply the new state.
    fn apply_sync_changes(&mut self, new_tracker_state: FixedBitSet, new_version: u64) {
        // Update local tracker bitset
        trace!("Applying sync changes");
        self.tracked_tables = new_tracker_state;
        self.last_sync_version = new_version;
    }
}

/// Connection abstraction that provides on possible implementation which uses the building
/// blocks ([`State`]) provided by this crate.
///
/// For simplicity, it takes ownership of an existing type which implements [`SqlExecutor`] and
/// initializes all the tracking infrastructure. The original type can still be accessed as
/// [`Connection`] implements both [`Deref`] and [`DerefMut`].
///
/// # Remarks
///
/// To make sure all changes are capture, it's recommended to always call
/// [`Connection::sync_watcher_tables()`]
/// before any query/statement or transaction.
///
/// # Example
///
/// ## Single Query/Statement
///
/// ```rust
/// use sqlite_watcher::connection::Connection;
/// use sqlite_watcher::connection::SqlExecutor;
/// use sqlite_watcher::watcher::Watcher;
///
/// pub fn track_changes<C:SqlExecutor>(connection: C) {
///     let watcher = Watcher::new().unwrap();
///     let mut connection = Connection::new(connection, watcher).unwrap();
///
///     // Sync tables so we are up to date.
///     connection.sync_watcher_tables().unwrap();
///
///     connection.sql_execute("sql query here").unwrap();
///
///     // Publish changes to the watcher
///     connection.publish_watcher_changes().unwrap();
/// }
/// ```
///
/// ## Transaction
///
/// ```rust
/// use sqlite_watcher::connection::Connection;
/// use sqlite_watcher::connection::{SqlExecutor};
/// use sqlite_watcher::watcher::Watcher;
///
/// pub fn track_changes<C:SqlExecutor>(connection: C) {
///     let watcher = Watcher::new().unwrap();
///     let mut connection = Connection::new(connection, watcher).unwrap();
///
///     // Sync tables so we are up to date.
///     connection.sync_watcher_tables().unwrap();
///
///     // Start a transaction
///     connection.sql_execute("sql query here").unwrap();
///     connection.sql_execute("sql query here").unwrap();
///     // Commit transaction
///
///     // Publish changes to the watcher
///     connection.publish_watcher_changes().unwrap();
/// }
/// ```
pub struct Connection<C: SqlExecutor> {
    state: State,
    watcher: Arc<Watcher>,
    connection: C,
}
impl<C: SqlExecutor> Connection<C> {
    /// Create a new connection with `connection` and `watcher`.
    ///
    /// See [`State::start_tracking()`] for more information about initialization.
    ///
    /// # Errors
    ///
    /// Returns error if the initialization failed.
    pub fn new(connection: C, watcher: Arc<Watcher>) -> Result<Self, C::Error> {
        let state = State::new();
        State::set_pragmas().execute(&connection)?;
        State::start_tracking().execute(&connection)?;
        Ok(Self {
            state,
            watcher,
            connection,
        })
    }

    /// Sync tables from the [`Watcher`] and update tracking infrastructure.
    ///
    /// See [`State::sync_tables()`] for more information.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to sync the changes to the database.
    pub fn sync_watcher_tables(&mut self) -> Result<(), C::Error> {
        self.state
            .sync_tables(&self.watcher)
            .execute(&self.connection)?;
        Ok(())
    }

    /// Check if any tables have changed and notify the [`Watcher`]
    ///
    /// See [`State::publish_changes()`] for more information.
    ///
    /// It is recommended to call this method
    ///
    /// # Errors
    ///
    /// Returns error if we failed to check for changes.
    pub fn publish_watcher_changes(&mut self) -> Result<(), C::Error> {
        self.state
            .publish_changes(&self.watcher)
            .execute(&self.connection)?;
        Ok(())
    }

    /// Disable all tracking on this connection.
    ///
    /// See [`State::stop_tracking`] for more details.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub fn stop_tracking(&mut self) -> Result<(), C::Error> {
        self.state
            .stop_tracking(&self.watcher)
            .execute(&self.connection)?;
        Ok(())
    }

    /// Consume the current connection and take ownership of the real sql connection.
    ///
    /// # Remarks
    ///
    /// This does not stop the tracking infrastructure enabled on the connection.
    /// Use [`Self::stop_tracking()`] to disable it first.
    pub fn take(self) -> C {
        self.connection
    }
}

/// Same as [`Connection`] but with an async executor.
#[allow(clippy::module_name_repetitions)]
pub struct ConnectionAsync<C: SqlExecutorAsync> {
    state: State,
    watcher: Arc<Watcher>,
    connection: C,
}
impl<C: SqlExecutorAsync> ConnectionAsync<C> {
    /// Create a new connection with `connection` and `watcher`.
    ///
    /// See [`State::start_tracking()`] for more information about initialization.
    ///
    /// # Errors
    ///
    /// Returns error if the initialization failed.
    pub async fn new(mut connection: C, watcher: Arc<Watcher>) -> Result<Self, C::Error> {
        let state = State::new();
        State::set_pragmas().execute_async(&mut connection).await?;
        State::start_tracking()
            .execute_async(&mut connection)
            .await?;
        Ok(Self {
            state,
            watcher,
            connection,
        })
    }

    /// See [`Connection::sync_watcher_tables`] for more details.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to sync the changes to the database.
    pub async fn sync_watcher_tables(&mut self) -> Result<(), C::Error> {
        self.state
            .sync_tables(&self.watcher)
            .execute_async(&mut self.connection)
            .await?;
        Ok(())
    }

    /// See [`Connection::publish_watcher_changes`] for more details.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to check for changes.
    pub async fn publish_watcher_changes(&mut self) -> Result<(), C::Error> {
        self.state
            .publish_changes(&self.watcher)
            .execute_async(&mut self.connection)
            .await?;
        Ok(())
    }

    /// See [`Connection::stop_tracking`] for more details.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn stop_tracking(&mut self) -> Result<(), C::Error> {
        self.state
            .stop_tracking(&self.watcher)
            .execute_async(&mut self.connection)
            .await?;
        Ok(())
    }

    /// Consume the current connection and take ownership of the real sql connection.
    ///
    /// # Remarks
    ///
    /// This does not stop the tracking infrastructure enabled on the connection.
    /// Use [`Self::stop_tracking()`] to disable it first.
    pub fn take(self) -> C {
        self.connection
    }
}

impl<C: SqlExecutorAsync> Deref for ConnectionAsync<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl<C: SqlExecutorAsync> DerefMut for ConnectionAsync<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl<C: SqlExecutorAsync> AsRef<C> for ConnectionAsync<C> {
    fn as_ref(&self) -> &C {
        &self.connection
    }
}

impl<C: SqlExecutorAsync> AsMut<C> for ConnectionAsync<C> {
    fn as_mut(&mut self) -> &mut C {
        &mut self.connection
    }
}

impl<C: SqlExecutor> Deref for Connection<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl<C: SqlExecutor> DerefMut for Connection<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl<C: SqlExecutor> AsRef<C> for Connection<C> {
    fn as_ref(&self) -> &C {
        &self.connection
    }
}

impl<C: SqlExecutor> AsMut<C> for Connection<C> {
    fn as_mut(&mut self) -> &mut C {
        &mut self.connection
    }
}

const TRACKER_TABLE_NAME: &str = "rsqlite_watcher_version_tracker";

const TRIGGER_LIST: [(&str, &str); 3] = [
    ("INSERT", "insert"),
    ("UPDATE", "update"),
    ("DELETE", "delete"),
];

#[inline]
fn create_tracking_table_query() -> String {
    format!(
        "CREATE TEMP TABLE IF NOT EXISTS `{TRACKER_TABLE_NAME}` (table_id INTEGER PRIMARY KEY, updated INTEGER)"
    )
}
#[inline]
fn empty_tracking_table_query() -> String {
    format!("DELETE FROM `{TRACKER_TABLE_NAME}`")
}
#[inline]
fn drop_tracking_table_query() -> String {
    format!("DROP TABLE IF EXISTS `{TRACKER_TABLE_NAME}`")
}

#[inline]
fn create_trigger_query(
    table_name: &str,
    trigger: &str,
    trigger_name: &str,
    table_id: usize,
) -> String {
    format!(
        r"
CREATE TEMP TRIGGER IF NOT EXISTS `{TRACKER_TABLE_NAME}_trigger_{table_name}_{trigger_name}` AFTER {trigger} ON `{table_name}`
BEGIN
    UPDATE  `{TRACKER_TABLE_NAME}` SET updated=1 WHERE table_id={table_id};
END
            "
    )
}

#[inline]
fn insert_table_id_into_tracking_table_query(id: usize) -> String {
    format!("INSERT INTO `{TRACKER_TABLE_NAME}` VALUES ({id},0)")
}

#[inline]
fn drop_trigger_query(table_name: &str, trigger_name: &str) -> String {
    format!("DROP TRIGGER IF EXISTS `{TRACKER_TABLE_NAME}_trigger_{table_name}_{trigger_name}`")
}

#[inline]
fn remove_table_id_from_tracking_table_query(table_id: usize) -> String {
    format!("DELETE FROM `{TRACKER_TABLE_NAME}` WHERE table_id={table_id}")
}

#[inline]
fn select_updated_tables_query() -> String {
    format!("SELECT table_id  FROM `{TRACKER_TABLE_NAME}` WHERE updated=1")
}

#[inline]
fn reset_updated_tables_query() -> String {
    format!("UPDATE `{TRACKER_TABLE_NAME}` SET updated=0 WHERE updated=1")
}

/// Create tracking triggers for `table` with `id`.
fn create_triggers(table: &str, id: usize) -> Vec<SqlExecuteStatement<String>> {
    TRIGGER_LIST
        .iter()
        .map(|(trigger, trigger_name)| {
            let query = create_trigger_query(table, trigger, trigger_name, id);
            SqlExecuteStatement::new(query)
        })
        .chain(std::iter::once_with(|| {
            let query = insert_table_id_into_tracking_table_query(id);
            SqlExecuteStatement::new(query)
        }))
        .collect()
}

/// Remove tracking triggers for `table` with `id`.
fn drop_triggers(table: &str, id: usize) -> Vec<SqlExecuteStatement<String>> {
    TRIGGER_LIST
        .iter()
        .map(|(_, trigger_name)| {
            let query = drop_trigger_query(table, trigger_name);
            SqlExecuteStatement::new(query)
        })
        .chain(std::iter::once_with(|| {
            let query = remove_table_id_from_tracking_table_query(id);
            SqlExecuteStatement::new(query)
        }))
        .collect()
}

/// Apply the new tracked table state to a [`State`].
struct ConcludeStateChangeStatement<'s> {
    state: &'s mut State,
    tracked_tables: FixedBitSet,
    new_version: u64,
}

impl Sealed for ConcludeStateChangeStatement<'_> {}
impl Statement for ConcludeStateChangeStatement<'_> {
    type Output = ();
    fn execute<S: SqlExecutor>(self, _: &S) -> Result<Self::Output, S::Error> {
        self.state
            .apply_sync_changes(self.tracked_tables, self.new_version);
        Ok(())
    }

    fn execute_mut<S: SqlExecutorMut>(self, _: &mut S) -> Result<Self::Output, S::Error> {
        self.state
            .apply_sync_changes(self.tracked_tables, self.new_version);
        Ok(())
    }

    async fn execute_async<S: SqlExecutorAsync>(self, _: &mut S) -> Result<Self::Output, S::Error> {
        self.state
            .apply_sync_changes(self.tracked_tables, self.new_version);
        Ok(())
    }
}

/// Calculate what the changes to be sent to the watcher.
struct CalculateWatcherUpdatesStatement<'s> {
    state: &'s State,
}

impl StatementWithInput for CalculateWatcherUpdatesStatement<'_> {
    type Input = Vec<u32>;
    type Output = FixedBitSet;

    fn execute<S: SqlExecutor>(self, _: &S, input: Self::Input) -> Result<Self::Output, S::Error> {
        Ok(self.state.prepare_watcher_changes(input))
    }
    fn execute_mut<S: SqlExecutorMut>(
        self,
        _: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        Ok(self.state.prepare_watcher_changes(input))
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        _: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        Ok(self.state.prepare_watcher_changes(input))
    }
}

/// Publish the changes to the watcher.
struct PublishWatcherChangesStatement<'w>(&'w Watcher);

impl Sealed for PublishWatcherChangesStatement<'_> {}

impl StatementWithInput for PublishWatcherChangesStatement<'_> {
    type Input = FixedBitSet;
    type Output = ();

    fn execute<S: SqlExecutor>(self, _: &S, input: Self::Input) -> Result<Self::Output, S::Error> {
        self.0.publish_changes(input);
        Ok(())
    }
    fn execute_mut<S: SqlExecutorMut>(
        self,
        _: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        self.0.publish_changes(input);
        Ok(())
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        _: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        self.0.publish_changes_async(input).await;
        Ok(())
    }
}

impl Sealed for SqlReadTableIdsStatement {}
struct SqlReadTableIdsStatement;
impl Statement for SqlReadTableIdsStatement {
    type Output = Vec<u32>;
    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        connection.sql_query_values(&select_updated_tables_query())
    }
    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        connection.sql_query_values(&select_updated_tables_query())
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        connection
            .sql_query_values(&select_updated_tables_query())
            .await
    }
}

/// It is possible on certain circumstances that if a connection can have leftover
/// tracking data that is not cleared. To make sure this is reset, we force empty
/// the table if we detect that we are not watching any tables at the moment.
struct MaybeResetResultsQuery;
impl StatementWithInput for MaybeResetResultsQuery {
    type Input = FixedBitSet;
    type Output = FixedBitSet;

    fn execute<S: SqlExecutor>(
        self,
        connection: &S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        if !input.is_clear() {
            // Reset updated values.
            connection.sql_execute(&reset_updated_tables_query())?;
        }
        Ok(input)
    }
    fn execute_mut<S: SqlExecutorMut>(
        self,
        connection: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        if !input.is_clear() {
            // Reset updated values.
            connection.sql_execute(&reset_updated_tables_query())?;
        }
        Ok(input)
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error> {
        if !input.is_clear() {
            // Reset updated values.
            connection
                .sql_execute(&reset_updated_tables_query())
                .await?;
        }
        Ok(input)
    }
}

#[cfg(test)]
mod test {
    use crate::connection::State;
    use crate::watcher::tests::new_test_observer;
    use crate::watcher::{ObservedTableOp, TableObserver, Watcher};
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, SyncSender};

    pub struct TestObserver {
        expected: Mutex<Vec<BTreeSet<String>>>,
        tables: Vec<String>,
        // Channel is here to make sure we don't trigger a merge of multiple pending updates.
        checked_channel: SyncSender<()>,
    }

    impl TestObserver {
        pub fn new(
            tables: Vec<String>,
            expected: impl IntoIterator<Item = BTreeSet<String>>,
        ) -> (Self, Receiver<()>) {
            let (sender, receiver) = std::sync::mpsc::sync_channel::<()>(0);
            let mut expected = expected.into_iter().collect::<Vec<_>>();
            expected.reverse();
            (
                Self {
                    expected: Mutex::new(expected),
                    tables,
                    checked_channel: sender,
                },
                receiver,
            )
        }
    }

    impl TableObserver for TestObserver {
        fn tables(&self) -> Vec<String> {
            self.tables.clone()
        }

        fn on_tables_changed(&self, tables: &BTreeSet<String>) {
            let expected = self.expected.lock().unwrap().pop().unwrap();
            assert_eq!(*tables, expected);
            self.checked_channel.send(()).unwrap();
        }
    }

    #[test]
    fn connection_state() {
        let service = Watcher::new().unwrap();

        let observer_1 = new_test_observer(["foo", "bar"]);
        let observer_2 = new_test_observer(["bar"]);
        let observer_3 = new_test_observer(["bar", "omega"]);

        let mut local_state = State::new();

        assert!(local_state.should_sync(&service).is_none());
        let observer_id_1 = service.add_observer(observer_1).unwrap();
        let foo_table_id = service.get_table_id("foo").unwrap();
        let bar_table_id = service.get_table_id("bar").unwrap();
        {
            let new_version = local_state
                .should_sync(&service)
                .expect("Should have new version");
            let (tracker, ops) = local_state
                .calculate_sync_changes(&service)
                .expect("must have changes");
            assert!(tracker[bar_table_id]);
            assert!(tracker[foo_table_id]);
            assert_eq!(ops.len(), 2);
            assert_eq!(
                ops[0],
                ObservedTableOp::Add("bar".to_string(), bar_table_id)
            );
            assert_eq!(
                ops[1],
                ObservedTableOp::Add("foo".to_string(), foo_table_id)
            );

            local_state.apply_sync_changes(tracker, new_version);
        }

        let observer_id_2 = service.add_observer(observer_2).unwrap();
        assert!(local_state.should_sync(&service).is_none());

        let observer_id_3 = service.add_observer(observer_3).unwrap();
        let omega_table_id = service.get_table_id("omega").unwrap();
        {
            let new_version = local_state
                .should_sync(&service)
                .expect("Should have new version");
            let (tracker, ops) = local_state
                .calculate_sync_changes(&service)
                .expect("must have changes");
            assert!(tracker[foo_table_id]);
            assert!(tracker[bar_table_id]);
            assert!(tracker[omega_table_id]);
            assert_eq!(ops.len(), 1);
            assert_eq!(
                ops[0],
                ObservedTableOp::Add("omega".to_string(), omega_table_id)
            );

            local_state.apply_sync_changes(tracker, new_version);
        }

        service.remove_observer(observer_id_2).unwrap();
        assert!(local_state.should_sync(&service).is_none());

        service.remove_observer(observer_id_3).unwrap();
        {
            let new_version = local_state
                .should_sync(&service)
                .expect("Should have new version");
            let (tracker, ops) = local_state
                .calculate_sync_changes(&service)
                .expect("must have changes");
            assert!(tracker[foo_table_id]);
            assert!(tracker[bar_table_id]);
            assert!(!tracker[omega_table_id]);
            assert_eq!(ops.len(), 1);
            assert_eq!(
                ops[0],
                ObservedTableOp::Remove("omega".to_string(), omega_table_id)
            );

            local_state.apply_sync_changes(tracker, new_version);
        }

        service.remove_observer(observer_id_1).unwrap();
        {
            let new_version = local_state
                .should_sync(&service)
                .expect("Should have new version");
            let (tracker, ops) = local_state
                .calculate_sync_changes(&service)
                .expect("must have changes");
            assert!(!tracker[foo_table_id]);
            assert!(!tracker[bar_table_id]);
            assert!(!tracker[omega_table_id]);
            assert_eq!(ops.len(), 2);
            assert_eq!(
                ops[1],
                ObservedTableOp::Remove("foo".to_string(), foo_table_id)
            );
            assert_eq!(
                ops[0],
                ObservedTableOp::Remove("bar".to_string(), bar_table_id)
            );

            local_state.apply_sync_changes(tracker, new_version);
        }
    }
}
