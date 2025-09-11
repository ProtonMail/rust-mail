//!
//! This module provides the main functionality for interacting with sqlite
//!
//! The primary point of interaction is the [`Stash`] struct, which is the
//! database pool which gives out [`Tether`]s.
//!
//! To interact with the database you first need to connect to a database by creating
//! a new [`Stash`], then obtain connections to it by obtaining tethers via [`Stash::connection`].
//! You can create transactions with [`Tether::transaction`] to obtain a [`Bond`],
//! which is the transaction type.
//!
//! Note that all 3 Stash, Bond and Tether are `Send`, but only Stash is Clone.
//! This is to avoid having one connection in two threads, which can result in deadlocks.
//! Under the bonnet, there is a background worker that manages the connection
//!
//!
//! The database handling uses the [`r2d2`] for connection pooling and [`rusqlite`]
//! to interface with sqlite.
//!

use crate::connection_manager::{
    StashConnectionPool, StashConnectionPoolError, StashPooledConnection,
};
use crate::orm::{ConversionError, DbRecord};
use anyhow::{Context, anyhow};
use core::fmt;
use core::fmt::Debug;
use core::future::Future;
use core::mem;
use core::ops::Deref;
use core::time::Duration;
use derivative::Derivative;
use flume::{Receiver as QueueReceiver, Sender as QueueSender, unbounded};
use indoc::formatdoc;
use proton_task_service::IntoNonPausableFuture;
use rusqlite::ffi::SQLITE_INTERRUPT;
use rusqlite::hooks::Action;
use rusqlite::types::FromSql;
use rusqlite::{
    Connection, Error as SqliteError, Rows, ToSql, Transaction, TransactionBehavior, ffi,
    params_from_iter,
};
use sqlite_watcher::connection::State;
use sqlite_watcher::statement::Statement;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use sqlite_watcher::watcher::TableObserver;
use sqlite_watcher::watcher::Watcher;
use std::any::Any;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::thread::{self};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tracing::{debug, error, trace};

/// Set a timeout for a specified amount of time when a table is locked. This
/// defaults to 5,000 milliseconds in the underlying libraries. This is currently only
/// expected to be triggered when the db is shared between different processes. We mediate the
/// access inside the same db process.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The maximum number of simultaneous connections allowed to the database.
const MAX_CONNECTIONS: u32 = 24;

#[derive(Debug)]
/// These are all the operations allowed on a tether.
enum Operation {
    /// Only the operations related to a transaction.
    Transaction(OperationTransaction),
    /// Only the operations related to execution
    Execution(OperationExec),
    /// Signal an interruption of execution.
    Interrupt,
    /// Quit the worker thread.
    Quit,
    /// Clean up any state when this connection is returned to the pool
    ReturnToPool,
}

/// Distinguishes transaction change detection behavior
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum TransactionTrackingPolicy {
    /// Use change tracking system.
    Tracking,
    /// Do not use change tracking system.
    Quiet,
}

/// Only the operations related to a transaction.
enum OperationTransaction {
    /// Starts a new transaction.
    Start(
        TransactionTrackingPolicy,
        OneshotSender<Result<(), StashError>>,
    ),

    /// Starts a new transaction.
    StartSync(BridgeClosure, TransactionTrackingPolicy),

    /// Commits a transaction, i.e. finalises it.
    Commit(
        TransactionTrackingPolicy,
        OneshotSender<Result<(), StashError>>,
    ),

    /// Rolls back a transaction, i.e. abandons it.
    Rollback(OneshotSender<Result<(), StashError>>),

    /// Used to bridge between async and sync code, Bond -> rusqlite::Transaction
    Bridge(BridgeClosure),

    /// Rollbacks a transaction too.
    /// This one is meant to be called in Bond's drop glue. That's why it doesn't have a sender.
    /// Same semantics as Rollback.
    RollbackAbort,
}

impl Debug for OperationTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(..) => write!(f, "Start"),
            Self::StartSync(..) => write!(f, "StartSync"),
            Self::Commit(..) => write!(f, "Commit"),
            Self::Rollback(_) => write!(f, "Rollback"),
            Self::Bridge(_) => write!(f, "Bridge"),
            Self::RollbackAbort => write!(f, "RollbackAbort"),
        }
    }
}

enum OperationExec {
    /// A query to be executed, where no results are expected. This is usually
    /// a write query, or a command, but differentiation is up to the caller and
    /// not enforced.
    Instruct(Instruction),

    /// A batch of queries to be executed, where no results are expected. This is
    /// usually migration commands, mutating the database schema.
    Batch(Batch),

    /// A query to be executed, where results are expected. This is typically a
    /// read query, but could be any query where results are expected, such as
    /// an `INSERT` query that returns the ID of the inserted row.
    Query(Query),

    /// This can be either a query or a transaction.
    Sync(SyncClosure),
}

impl Debug for OperationExec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Instruct(_) => write!(f, "Instruct"),
            Self::Batch(_) => write!(f, "Batch"),
            Self::Query(_) => write!(f, "Query"),
            Self::Sync(_) => write!(f, "Sync"),
        }
    }
}

/// Error type for the [`Stash`] module.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StashError {
    /// There was a problem with deserialising the query results.
    /// This is either a serde error or there's a mismatch between the record and the DB table.
    #[error("Query results deserialization error: {0}")]
    DeserializationError(#[from] ConversionError),

    // TODO: have a better from impl.
    //
    /// There was a problem with statement execution.
    /// Note that this refers to executing a prepared statement,
    /// e.g. actually running a query, and not the process of preparing the statement/query.
    #[error("Statement execution error: {0}")]
    ExecutionError(#[from] SqliteError),

    #[error("Trying to update a record that hasn't been saved yet")]
    IdNotSet,

    /// An operation requiring a transaction was attempted, such as a commit or
    /// rollback, but no active transaction was found.
    #[error("No active transaction")]
    NoActiveTransaction,

    /// There was a problem when parsing and validating a statement.
    /// Note that this refers to preparing a statement from a query and parameters, prior to execution.
    #[error("Error while parsing the query: {0}")]
    PreparationError(SqliteError),

    #[error("Transaction error: {0}")]
    TransactionError(SqliteError),

    /// There was a problem with subscriptions. For some reason the subscription
    /// has ended up in the wrong place. This should never happen in practice.
    #[error("Watcher error: `{0}`")]
    WatcherError(String),

    /// No rows were updated upon saving a record. This can happen if the record
    /// data hasn't changed, in which case it's not an error — but in other
    /// situations, it would signify a problem.
    #[error("No rows updated upon saving record")]
    NoRowsUpdated,

    #[error("The query did not affect any row.")]
    QueryReturnedNoRows,

    /// Critical internal error that cannot be recovered from.
    #[error("Critical internal stash error: {0}")]
    Critical(#[from] anyhow::Error),

    #[error("Failed to acquire connection in the given time limit")]
    ConnectionAcquireTimedOut,

    /// Custom variant that is not critical
    #[error("{0}")]
    Custom(anyhow::Error),
}

pub type StashResult<T> = Result<T, StashError>;
pub type RusqliteResult<T> = Result<T, SqliteError>;

impl StashError {
    pub fn interrupted() -> Self {
        StashError::ExecutionError(SqliteError::SqliteFailure(
            ffi::Error::new(SQLITE_INTERRUPT),
            None,
        ))
    }
    pub fn was_interrupt(&self) -> bool {
        match self {
            StashError::ExecutionError(SqliteError::SqliteFailure(err, _))
            | StashError::PreparationError(SqliteError::SqliteFailure(err, _))
            | StashError::TransactionError(SqliteError::SqliteFailure(err, _)) => {
                err.code == rusqlite::ErrorCode::OperationInterrupted
            }
            _ => false,
        }
    }
}

/// An operation to be executed by the worker, which does not return any data.
///
/// This is used for operations such as `INSERT`, `UPDATE`, and `DELETE`, where
/// the result is the number of rows affected, along with other similar
/// commands.
///
#[derive(Derivative)]
#[derivative(Debug)]
struct Instruction {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    #[derivative(Debug = "ignore")]
    sender: OneshotSender<Result<usize, StashError>>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    #[derivative(Debug = "ignore")]
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl Instruction {
    /// Prepares and executes a query, and returns the number of affected rows.
    fn run(&self, connection: &Connection) -> Result<usize, StashError> {
        let mut statement = connection
            .prepare_cached(&self.query)
            .map_err(StashError::PreparationError)?;
        let affected = statement
            .execute(params_from_iter(&self.params))
            .map_err(StashError::ExecutionError)?;
        // I'm not sure if we should do this.
        // TODO : Put this behind a feature flag (next MR)
        if let Some(query) = statement.expanded_sql() {
            trace!("Query: {query}");
        }
        Ok(affected)
    }
}

/// A batch operation to be executed by worker. Similarly to [`Instruction`] it does
/// not return any data.
///
/// It is used for batch operations not requiring returned results, such as migrating
/// database schema.
///
#[derive(Derivative)]
#[derivative(Debug)]
struct Batch {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    #[derivative(Debug = "ignore")]
    sender: OneshotSender<Result<(), StashError>>,

    /// The queries to execute separated by `;`.
    /// It takes no parameter substitutions.
    queries: String,
}

impl Batch {
    /// Executes a query
    fn run(&self, connection: &Connection) -> Result<(), StashError> {
        connection
            .execute_batch(&self.queries)
            .map_err(StashError::ExecutionError)
    }
}

/// A notification that a change has been made to the database.
///
/// This struct is used to inform any subscribers that changes have been made to
/// the database. It is used by the central background worker to notify any
/// subscribers that have registered interest in such notifications.
///
/// # See also
///
/// * [`Stash::subscribe()`]
///
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// The action that has been performed on the table. This can be one of
    /// `INSERT`, `UPDATE`, or `DELETE`.
    pub action: Action,

    /// The name of the table that the action was performed on, i.e. that has
    /// changed.
    pub table: String,

    /// The row ID of the row that has been acted on, i.e. changed. This may or
    /// may not be useful.
    pub row: u64,

    /// The id of the associated connection
    pub id: u64,
}

type QueryResult = Box<dyn Any + Send + 'static>;

/// An operation to be executed by the worker, which returns data.
///
/// This is used for operations such as `SELECT`, where the result is a set of
/// rows of data. Notably, the deserialisation function is also passed, so that
/// the results can be converted into the desired type. This is because the
/// [`Rows`] type returned by the [`rusqlite`] library is not thread-safe.
///
#[derive(Derivative)]
#[derivative(Debug)]
struct Query {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    #[derivative(Debug = "ignore")]
    sender: OneshotSender<Result<QueryResult, StashError>>,

    /// The deserialisation function to use to convert the query results into
    /// the desired type. This is necessary because the [`Rows`] type returned
    /// by the [`rusqlite`] library is not thread-safe.
    #[derivative(Debug = "ignore")]
    converter: Box<dyn FnOnce(Rows<'_>) -> QueryResult + Send + 'static>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    #[derivative(Debug = "ignore")]
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl Query {
    /// Prepares and executes a query, and returns any rows of data emitted.
    fn run_and_send(self, connection: &Connection) {
        let params = params_from_iter(&self.params);
        let mut stmt = match connection.prepare_cached(&self.query) {
            Ok(stmt) => stmt,
            Err(e) => {
                _ = self.sender.send(Err(StashError::PreparationError(e)));
                return;
            }
        };

        if tracing::enabled!(tracing::Level::DEBUG)
            && let Some(query) = stmt.expanded_sql()
        {
            debug!("Query: {query}");
        }

        let res = match stmt.query(params) {
            Ok(val) => Ok((self.converter)(val)),
            Err(e) => Err(StashError::ExecutionError(e)),
        };

        _ = self.sender.send(res);
    }
}

/// Configuration used to create stash database pool.
///
#[derive(Default, Clone, Copy)]
pub struct StashConfiguration<'a> {
    /// The path to the SQLite database file. If `None`, an in-memory
    /// database is created.
    pub path: Option<&'a Path>,
    /// How many connections are used. If `None`, [`MAX_CONNECTIONS`] is used.
    pub pool_size: Option<u32>,
    /// How many idle connections are allowed to be maintained before new connections are established
    /// For `None` the default of [`IDLE_CONNECTIONS`]  or [`pool_size`] will be used whichever is lower.
    pub idle_count: Option<u32>,
}

impl<'a> StashConfiguration<'a> {
    pub fn test() -> Self {
        Self {
            idle_count: Some(0),
            ..Default::default()
        }
    }

    pub fn test_with_path(path: &'a Path) -> Self {
        let mut config = Self::test();
        config.path = Some(path);
        config
    }
}

impl<'a> From<Option<&'a PathBuf>> for StashConfiguration<'a> {
    fn from(value: Option<&'a PathBuf>) -> Self {
        Self {
            path: value.map(|v| &**v),
            ..Default::default()
        }
    }
}

/// This is stash's database pool. Its main use is to create [`Tether`]s.
// Internally this spawns a task that handles all of the operations (See [`StashOperation`]).
#[derive(Clone)]
pub struct Stash {
    /// The [`Watcher`] instance for the [`Stash`], which is used to monitor the
    /// database for changes and notify subscribers. This is used to provide
    /// real-time updates to any subscribers that have registered interest in
    /// changes to the database for given tables.
    watcher: Arc<Watcher>,

    /// The pool used for database connections.
    pool: Arc<StashConnectionPool>,

    tx_lock: Arc<Mutex<()>>,
}

impl Debug for Stash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut r = f.debug_struct("Stash");

        r.finish_non_exhaustive()
    }
}

impl Stash {
    /// Creates a new [`Stash`] instance.
    ///
    /// This function creates a new [`Stash`] instance with an associated
    // background worker on a separate task, with a new SQLite connection
    // pool.
    //
    // Note that the pool is created internally by the worker, and fully
    // managed by it, as there can only be one worker per [`Stash`] instance
    // and database operations need to be executed sequentially.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem creating
    /// the database or connection pool.
    ///
    pub fn new<'a>(config: impl Into<StashConfiguration<'a>>) -> Result<Self, StashError> {
        let watcher = Watcher::new().map_err(|e| StashError::WatcherError(e.to_string()))?;
        let pool = Self::make_pool(config.into(), &watcher)?;
        Ok(Self {
            pool,
            watcher,
            tx_lock: Default::default(),
        })
    }

    /// Create a sqlite pool.
    /// This is infallible, if it cannot open the file it will fail later on when we try to
    /// connect.
    fn make_pool(
        config: StashConfiguration<'_>,
        watcher: &Arc<Watcher>,
    ) -> Result<Arc<StashConnectionPool>, StashError> {
        let StashConfiguration {
            path, pool_size, ..
        } = config;

        match path {
            Some(p) => debug!("New Stash with file: {:?}", p),
            None => debug!("New Stash with in-memory database"),
        }

        let max_connections = pool_size.unwrap_or(MAX_CONNECTIONS) as usize;

        let init_fn = Box::new(|c: &mut Connection| {
            c.execute_batch(&formatdoc!("
                        PRAGMA journal_mode = WAL;         -- Better write-concurrency
                        PRAGMA synchronous = NORMAL;       -- Perform fsync only at critical points
                        PRAGMA wal_checkpoint(TRUNCATE);   -- Free space by truncating WAL files from the last run
                        PRAGMA busy_timeout = {};          -- Wait if the database is busy/locked
                        PRAGMA foreign_keys = ON;          -- Enforce foreign key constraints
                        PRAGMA temp_store = MEMORY;        -- Allows temporary storage for watcher
                        PRAGMA recursive_triggers='ON';    -- Allows recursive triggers for watcher
                        PRAGMA page_size = 8192;
                        PRAGMA cache_size = 10000;
                    ", BUSY_TIMEOUT.as_millis()))?;
            // Ensure on iOS wall checkpointing is disabled on close. We could have set this
            // up as a configuration option, but we may forget to set this correctly in the
            // future and re-introduce this bug.
            #[cfg(target_os = "ios")]
            if !c.set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )? {
                return Err(SqliteError::UserFunctionError(
                    "Failed to set wal checkpoint on close".into(),
                ));
            }
            Ok(())
        });

        match path {
            Some(p) => StashConnectionPool::file(p, max_connections, init_fn, watcher)
                .map_err(StashError::ExecutionError),
            None => StashConnectionPool::tmp_file(max_connections, init_fn, watcher)
                .map_err(StashError::ExecutionError),
        }
    }

    /// Gets a connection from the pool.
    ///
    /// This function gets a connection from the pool. The connection context is
    /// returned as a [`Tether`], which is a thread-safe handle to the
    /// connection itself. The underlying connection is automatically returned
    /// to the pool when the [`Tether`] goes out of scope.
    ///
    /// In practice it should not be necessary to call this method in normal
    /// day-to-day operation, as the [`Stash`] struct provides all the necessary
    /// functionality. It is provided for completeness and for any cases where
    /// direct access to a database connection is required. The most common case
    /// for needing a persistent connection is when using transactions, and
    /// starting a new transaction returns a new [`Tether`] instance to use.
    ///
    /// # Errors
    ///
    /// Note that this function is infallible. That's because the allocation of
    /// the [`Tether`] and its internal handle, and the association of that
    /// handle to an actual connection, do not occur at the same time. The
    /// connection is only created/obtained when the first query using the
    /// [`Tether`] is executed. Therefore, the [`Tether`] itself is not
    /// associated with any connection at the time of creation, and so cannot
    /// fail. The reason for this design is so that "connections" can be created
    /// quickly, with no delay, instead of waiting to be processed by the queue.
    /// As query execution requires handling of errors anyway, this does not
    /// introduce any additional burden, and streamlines connection handling.
    ///
    /// # See also
    ///
    /// * [`Stash::transaction()`]
    /// * [`Tether::transaction()`]
    ///
    pub async fn connection(&self) -> Result<Tether, StashError> {
        Tether::new(self).await
    }

    /// Subscribes to notifications of changes to a specific table.
    ///
    /// # Errors
    ///
    /// See [`Stash::subscribe()`].
    pub fn subscribe_to<F>(&self, observer: F) -> Result<WatcherHandle, StashError>
    where
        F: FnOnce(QueueSender<()>) -> Box<dyn TableObserver>,
    {
        let (sender, receiver) = unbounded();
        let handle = self
            .watcher
            .add_observer_with_drop_remove(observer(sender))
            .map_err(|e| {
                StashError::WatcherError(format!(
                    "Could not observe requested table, details: `{e}`"
                ))
            })?;

        Ok(WatcherHandle { receiver, handle })
    }

    /// Interrupt all ongoing queries and transactions.
    ///
    /// This method will also prevent new transactions from executing until [`resume()`] is called.
    pub fn interrupt(&self) {
        self.pool.interrupt();
    }

    /// Resume execution of transactions after a call to [`interrupt()`].
    pub fn resume(&self) {
        self.pool.resume();
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct WatcherHandle {
    pub receiver: QueueReceiver<()>,
    pub handle: DropRemoveTableObserverHandle,
}

impl WatcherHandle {
    pub fn new(receiver: QueueReceiver<()>, handle: DropRemoveTableObserverHandle) -> Self {
        Self { receiver, handle }
    }

    pub async fn next(&self) -> Result<(), flume::RecvError> {
        self.receiver.recv_async().await
    }
}

/// A connection to the database. It is used to execute queries against the database, and obtained
/// from [`Stash::connection`].
///
/// # Design
///
/// Because [`PooledConnection`] is not [`Send`] compatible, it cannot be passed
/// between threads, and so cannot cross the async boundary. This is an
/// inherited limitation of the [`rusqlite`] crate.
/// `stash` works around it by using the actor pattern and wrapping each connection in a
/// thread, using message passing for executing the queries and waiting for the result.
pub struct Tether {
    connection: StashPooledConnection,

    watcher: Arc<Watcher>,

    tx_lock: Arc<Mutex<()>>,
}

impl Tether {
    /// Subscribes to notifications of changes to a specific table.
    ///
    /// # Errors
    ///
    /// See [`Stash::subscribe()`].
    pub fn subscribe_to<F>(&self, observer: F) -> Result<WatcherHandle, StashError>
    where
        F: Fn(QueueSender<()>) -> Box<dyn TableObserver>,
    {
        let (sender, receiver) = unbounded();
        let handle = self
            .watcher
            .add_observer_with_drop_remove(observer(sender))
            .map_err(|e| {
                StashError::WatcherError(format!(
                    "Could not observe requested table, details: `{e}`"
                ))
            })?;

        Ok(WatcherHandle { receiver, handle })
    }
    /// Runs a query and returns the affected row count.
    ///
    /// This function prepares a query and executes it on the database, and then
    /// indicates whether it was successful, returning the number of affected
    /// rows. It does not return any rows of data that the query may have
    /// emitted, and is designed for situations where no data is expected, such
    /// as `INSERT`, `UPDATE`, or `DELETE` queries.
    ///
    /// Note that the [`params!`](crate::utils::params) macro is available to
    /// help shorten the syntax for passing in the query parameters.
    ///
    /// # Read vs write
    ///
    /// Although this function is *designed* for write queries, this is implied
    /// and a convenience, and it is entirely possible to use it for read
    /// queries as well — but that would usually be of little benefit. The
    /// number of rows affected will be zero for read queries. Any semantic
    /// difference between read and write queries is left to the caller to
    /// decide, and does not result in any difference in handling by this
    /// module. The [`rusqlite`] library will handle the distinction as
    /// necessary.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`ExecutionError`](StashError::ExecutionError) - Problem executing
    ///     the query.
    ///   - [`OneShotError`](StashError::OneShotError) - Problem receiving data
    ///     back to the caller via the oneshot channel.
    ///   - [`QueueError`](StashError::QueueError) - Problem sending the
    ///     operation to the queue.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    pub async fn execute<Q: Into<String>>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<usize, StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation = Operation::Execution(OperationExec::Instruct(Instruction {
            sender,
            params,
            query: query.into(),
        }));
        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")
    }

    /// Runs batch of queries separated by `;`. It does not return any result.
    ///
    /// It is designed for situations such as schema migrations where there are multiple commands
    /// without the need for any result.
    ///
    /// # Parameters
    ///
    /// * `query` - The batch of queries separated by `;`.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`ExecutionError`](StashError::ExecutionError) - Problem executing
    ///     the query.
    ///   - [`OneShotError`](StashError::OneShotError) - Problem receiving data
    ///     back to the caller via the oneshot channel.
    ///   - [`QueueError`](StashError::QueueError) - Problem sending the
    ///     operation to the queue.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    pub async fn batch<Q: Into<String>>(&self, queries: Q) -> Result<(), StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation = Operation::Execution(OperationExec::Batch(Batch {
            sender,
            queries: queries.into(),
        }));
        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")
    }

    /// Runs a query and returns any rows of data emitted.
    ///
    /// This function prepares a query and executes it on the database, and
    /// returns the resulting rows of data as a collection of instances of the
    /// specified `T` type, where `T` is any concrete type implementing the
    /// [`DbRecord`] trait. The requirement to formalise the return type
    /// streamlines the process of handling the results.
    pub async fn query<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String>,
        T: DbRecord + Send + 'static,
    {
        let converter = move |mut rows: Rows<'_>| {
            let mut results = vec![];
            while let Some(row) = rows.next()? {
                results.push(T::from_row(row)?);
            }
            Ok::<_, ConversionError>(results)
        };

        Ok(self.do_query(query, params, converter).await??)
    }

    pub async fn do_query<T>(
        &self,
        query: impl Into<String>,
        params: Vec<Box<dyn ToSql + Send>>,
        convert: impl Send + 'static + FnOnce(Rows<'_>) -> T,
    ) -> Result<T, StashError>
    where
        T: Send + 'static,
    {
        let convert =
            move |rows: Rows<'_>| Box::new(convert(rows)) as Box<dyn Any + Send + 'static>;

        let (sender, receiver) = oneshot::channel();
        let query = Query {
            sender,
            converter: Box::new(convert),
            params,
            query: query.into(),
        };
        let operation = Operation::Execution(OperationExec::Query(query));
        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;

        let item = receiver
            .await
            .expect("Tether closed its channel with handles still open")?;
        //
        // The type we receive back is described as Any so that it can pass through
        // the channel without introducing unnecessary type constraints, but is in
        // fact already known to be of type T, so we can downcast it safely.
        Ok(*item.downcast::<T>().unwrap())
    }

    /// Utility function to return rows of a singular type.
    ///
    /// This function is similar in nature to [`Interface::query()`] but it
    /// returns values that implement [`FromSql`] and [`ToSql`] rather
    /// than [`DbRecord`]. This is functionally equivalent to writing the
    /// following snippet:
    ///
    /// ```skip
    ///  #[derive(DbRecord, Debug,Eq,PartialEq)
    ///  struct RecordValue<T:FromSql> {
    ///     #[DbField]
    ///     value:T
    ///  }
    ///
    ///  let values:Vec<RecordValue<T>> = interface.query(
    ///         "SELECT number FROM table",
    ///         vec![]).await.unwrap();
    /// ```
    ///
    /// # Query structure
    ///
    /// This utility function requires all the queries to return only one value
    /// named `value` or the conversion will not be successful.
    ///
    /// # Errors
    ///
    /// See [`Interface::query`] for more information.
    ///
    /// # Example
    ///
    /// ```
    /// use stash::params;
    /// use stash::stash::Tether;
    ///
    /// async fn value_query(tether:&Tether) {
    ///     let values:Vec<f64> = tether.query_values(
    ///         "SELECT number FROM table",
    ///         vec![]).await.unwrap();
    /// }
    /// ```
    ///
    /// # See also
    ///
    /// * [`Interface::query`]
    ///
    pub async fn query_values<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String> + Send,
        T: Clone + Debug + FromSql + PartialEq + Send + Sync + ToSql + 'static,
    {
        self.query::<_, ValueRecord<T>>(query, params)
            .await
            .map(|values| values.into_iter().map(|v| v.value).collect())
    }

    /// Utility function to return a single row of a singular type.
    /// See [`Tether::query_values()`] for more information
    ///
    /// # Errors
    ///
    /// If no rows are returned, this function returns
    /// [`SqliteError::QueryReturnedNoRows`].
    ///
    pub async fn query_value<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<T, StashError>
    where
        Q: Into<String> + Send,
        T: Clone + Debug + FromSql + PartialEq + Send + Sync + ToSql + 'static,
    {
        Self::query_value_opt(self, query, params)
            .await?
            .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))
    }

    /// Utility function to return a single row of a singular type.
    /// See [`Tether::query_values()`] for more information
    pub async fn query_value_opt<T>(
        &self,
        query: impl Into<String>,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Option<T>, StashError>
    where
        T: Clone + Debug + FromSql + PartialEq + Send + Sync + ToSql + 'static,
    {
        let mut values = self.query_values::<_, T>(query.into(), params).await?;
        match values.len() {
            0 => Ok(None),
            1 => Ok(values.pop()),
            _ => Err(StashError::Critical(anyhow!(
                "Query returned multiple rows"
            ))),
        }
    }

    /// Note that under the current design, transactions are not nestable, and
    /// each transaction must be carried out on its own, independent connection.
    /// It is possible to reuse a connection for multiple transactions, but only
    /// one transaction can be active at a time on a given connection.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`OneShotError`](StashError::OneShotError) - Problem receiving data
    ///     back to the caller via the oneshot channel.
    ///   - [`QueueError`](StashError::QueueError) - Problem sending the
    ///     operation to the queue.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///   - [`TransactionAlreadyStarted`](StashError::TransactionAlreadyStarted) -
    ///     A new transaction cannot be started because one is already active on
    ///     this connection.
    ///   - [`TransactionError`](StashError::ExecutionError) - Problem starting
    ///     the transaction.
    ///
    ///
    pub async fn tx<F, T, E>(&mut self, closure: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, E>,
        E: From<StashError>,
    {
        self.tx_impl(TransactionTrackingPolicy::Tracking, closure)
            .await
    }

    /// The transaction will produce no any notifications.
    ///
    /// This method is used to start a transaction without listening for changes.
    /// It is needed for internal implementation of the watch mechanism and scrollers.
    ///
    /// # Errors
    ///
    /// see [`Tether::tx()`]
    pub async fn quiet_tx<F, T, E>(&mut self, closure: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, E>,
        E: From<StashError>,
    {
        self.tx_impl(TransactionTrackingPolicy::Quiet, closure)
            .await
    }

    async fn tx_impl<F, T, E>(
        &mut self,
        policy: TransactionTrackingPolicy,
        closure: F,
    ) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, E>,
        E: From<StashError>,
    {
        // We acquire a lock rather than relying on the SQLite internal lock as it allows us to:
        // * Avoid busy timeouts in the same process
        // * Ensure that when this is running on a pausable future, that we _really_ only create
        //   a new transaction if we are not paused. Previously it would be possible for many
        //   transactions to be in flight at the same time.
        let tx_lock = self.tx_lock.clone();
        let _guard = tx_lock.lock().await;
        async {
            let tx = self.transaction_impl(policy).await?;
            let r = closure(&tx).await;
            if r.is_err() {
                if let Err(e) = tx.rollback().await {
                    error!("Failed to rollback transaction: {e:?}");
                }
                return r;
            }
            tx.commit_(policy)
                .await
                .inspect_err(|e| error!("Failed to commit transaction: {e:?}"))?;
            r
        }
        .into_non_pausable()
        .await
    }

    async fn transaction_impl(
        &mut self,
        policy: TransactionTrackingPolicy,
    ) -> Result<Bond<'_>, StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation = Operation::Transaction(OperationTransaction::Start(policy, sender));

        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))??;

        Ok(Bond::new(self))
    }

    /// Starts a new tethered worker thread.
    ///
    /// This function creates a new [`TetheredWorker`] instance associated to a
    /// SQLite connection pool, and starts the worker. This is run in a separate
    /// thread that is used to run blocking code, so it can execute queries in a
    /// non-blocking manner. The worker will execute queries sequentially, as
    /// they are received, and return the results via oneshot channels. In this
    /// way, it is very similar to the main worker, but is connection-specific.
    ///
    async fn new(stash: &Stash) -> Result<Self, StashError> {
        let pool = stash.pool.clone();
        let connection = tokio::task::spawn_blocking(move || {
            pool.acquire(None).map_err(|e| match e {
                StashConnectionPoolError::Connection(e) => StashError::ExecutionError(e),
                StashConnectionPoolError::TimedOut => StashError::ConnectionAcquireTimedOut,
            })
        })
        .await
        .map_err(|e| StashError::Custom(anyhow!("Failed to join blocking task: {e}")))??;
        Ok(Self {
            connection,
            watcher: stash.watcher.clone(),
            tx_lock: Arc::clone(&stash.tx_lock),
        })
    }

    pub async fn sync_query<T: Send + 'static>(
        &self,
        callback: impl FnOnce(&rusqlite::Connection) -> Result<T, StashError> + Send + 'static,
    ) -> Result<T, StashError> {
        let span = tracing::Span::current();
        let closure = Box::new(move |conn: &rusqlite::Connection| {
            let _g = span.enter();
            callback(conn).map(|x| Box::new(x) as Box<dyn Any + Send>)
        });

        let (sender, receiver) = oneshot::channel();
        let sync_closure = SyncClosure { closure, sender };
        let operation = Operation::Execution(OperationExec::Sync(sync_closure));

        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        let ret = receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        // This cannot fail as the type system assures us that the return type of `callback` is T
        ret.map(|x| *x.downcast().expect("Downcast failed?"))
    }

    pub async fn sync_tx(
        &mut self,
        callback: impl FnOnce(&rusqlite::Transaction<'_>) -> StashResult<()> + Send + 'static,
    ) -> StashResult<()> {
        self.sync_tx_returning(callback).await
    }

    pub async fn sync_tx_returning<T: Send + 'static>(
        &mut self,
        callback: impl FnOnce(&rusqlite::Transaction) -> StashResult<T> + Send + 'static,
    ) -> StashResult<T> {
        self.run_sync_tx(callback, TransactionTrackingPolicy::Tracking)
            .await
    }

    /// This runs the given callback in the tether thread.
    async fn run_sync_tx<T: Send + 'static>(
        &mut self,
        callback: impl FnOnce(&rusqlite::Transaction<'_>) -> StashResult<T> + Send + 'static,
        policy: TransactionTrackingPolicy,
    ) -> StashResult<T> {
        let tx_lock = self.tx_lock.clone();
        let _guard = tx_lock.lock().await;
        let span = tracing::Span::current();
        let closure = Box::new(move |tx: &rusqlite::Transaction| {
            let _g = span.enter();
            callback(tx).map(|x| Box::new(x) as Box<dyn Any + Send>)
        });

        let (sender, receiver) = oneshot::channel();
        let sync_closure = BridgeClosure { closure, sender };
        let operation =
            Operation::Transaction(OperationTransaction::StartSync(sync_closure, policy));

        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        let ret = receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        // This cannot fail as the type system assures us that the return type of `callback` is T
        ret.map(|x| *x.downcast().expect("Downcast failed?"))
    }
}

impl Debug for Tether {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tether").finish_non_exhaustive()
    }
}

impl Drop for Tether {
    fn drop(&mut self) {
        let _ = self.connection.send(Operation::ReturnToPool);
    }
}

pub(crate) struct PooledTether {
    sender: flume::Sender<Operation>,
}
impl PooledTether {
    pub(crate) fn new(
        connection: Connection,
        watcher: &Arc<Watcher>,
        pool: Weak<StashConnectionPool>,
        number: usize,
    ) -> Self {
        // One for tether commands, another for interruption
        let (sender, receiver) = flume::bounded(2);

        let watcher_cloned = watcher.clone();
        let pool_cloned = pool.clone();
        thread::Builder::new()
            .name(format!("Tether Worker {number:02}"))
            .spawn(move || {
                Self::thread_loop(connection, receiver, watcher_cloned.as_ref(), pool_cloned);
            })
            .expect("Failed to create named thread, please fix me");

        Self { sender }
    }

    pub(crate) fn interrupt_notifier(&self) -> PooledTetherInterruptNotifier {
        PooledTetherInterruptNotifier(self.sender.clone())
    }

    fn thread_loop(
        connection: Connection,
        receiver: flume::Receiver<Operation>,
        watcher: &Watcher,

        pool: Weak<StashConnectionPool>,
    ) {
        let mut sm = TetheredWorkerStateMachine {
            transaction: None,
            connection: &connection,
            state: State::new(),
            watcher,
            was_interrupted: false,
        };

        while let Ok(operation) = receiver.recv() {
            let Some(pool) = pool.upgrade() else {
                break;
            };
            if sm.handle_operation(operation, &pool) {
                break;
            }
        }
        sm.handle_close();
    }

    fn send(&self, operation: Operation) -> Result<(), flume::SendError<Operation>> {
        self.sender.send(operation)
    }
}

impl Drop for PooledTether {
    fn drop(&mut self) {
        let _ = self.sender.send(Operation::Quit);
    }
}

pub(crate) struct PooledTetherInterruptNotifier(flume::Sender<Operation>);

impl PooledTetherInterruptNotifier {
    pub fn interrupt(&self) {
        let _ = self.0.send(Operation::Interrupt);
    }
}

// PERF: Monomorphic SyncClosure for common use cases like () and usize.
type SyncClosureRetTy = Result<Box<dyn Any + Send>, StashError>;
struct SyncClosure {
    closure: Box<dyn FnOnce(&Connection) -> SyncClosureRetTy + Send>,
    sender: OneshotSender<SyncClosureRetTy>,
}

struct BridgeClosure {
    closure: Box<dyn FnOnce(&Transaction) -> SyncClosureRetTy + Send>,
    sender: OneshotSender<SyncClosureRetTy>,
}

/// Database transaction context.
///
/// This struct provides a lightweight, thread-safe database transaction context
/// — which is not an actual transaction, but a tether to one — that can be
/// shared easily and without concern. It is used to execute queries against the
/// database,
///
///
/// # Design
///
/// Its design resolves around being a wrapper around a mutable referance to [`Tether`] instance,
/// to provide dedicated Transaction type. This type is meant to be required for any
/// database modification queries to ensure safety of execution. Rust type system ensures that
/// there is only one transaction per tether.
///
/// # Errors
/// Any modification query run of the scope of the Transaction may trigger `Database is busy` error.
///
/// # See also
///
/// * [`Tether`]
///
#[derive(Debug)]
pub struct Bond<'tether> {
    /// The associated [`Tether`] instance.
    tether: &'tether mut Tether,
}

impl<'tether> Bond<'tether> {
    /// Create new instance of the Bond.
    ///
    fn new(tether: &'tether mut Tether) -> Self {
        Self { tether }
    }

    #[allow(clippy::mem_forget)]
    /// Internal commit implementation.
    ///
    /// This method is used to commit a transaction without publishing changes.
    /// It is needed for internal implementation of the watch mechanism and scrollers.
    ///
    /// # Errors
    ///
    /// see [`Bond::commit()`]
    ///
    async fn commit_(
        self,
        transaction_policy: TransactionTrackingPolicy,
    ) -> Result<(), StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation =
            Operation::Transaction(OperationTransaction::Commit(transaction_policy, sender));
        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;

        if let Err(e) = receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))?
        {
            error!("Commit error: {e:}");
            self.rollback().await?;
            return Ok(());
        }
        // Transaction commited, skip the drop logic
        mem::forget(self);

        Ok(())
    }

    /// Rolls back a transaction.
    ///
    /// This function rolls back, i.e. abandons, an existing, active
    /// transaction.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`NoActiveTransaction`](StashError::NoActiveTransaction) - No
    ///     transaction is currently active on this connection.
    ///   - [`OneShotError`](StashError::OneShotError) - Problem receiving data
    ///     back to the caller via the oneshot channel.
    ///   - [`QueueError`](StashError::QueueError) - Problem sending the
    ///     operation to the queue.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///   - [`TransactionError`](StashError::ExecutionError) - Problem starting
    ///     the transaction.
    ///
    #[allow(clippy::mem_forget)]
    async fn rollback(self) -> Result<(), StashError> {
        let this = ManuallyDrop::new(self); // The drop glue does an implicit rollback
        let (sender, receiver) = oneshot::channel();

        let operation = Operation::Transaction(OperationTransaction::Rollback(sender));
        this.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))??;

        Ok(())
    }

    /// This function will execute `callback` in the thread that holds the transaction.
    ///
    /// Useful to speed up slow logic and to get better ergonomics.
    pub async fn sync_bridge<T: Send + 'static>(
        &self,
        callback: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, StashError> + Send + 'static,
    ) -> Result<T, StashError> {
        let span = tracing::Span::current();
        let closure = Box::new(move |conn: &rusqlite::Transaction| {
            let _g = span.enter();
            callback(conn).map(|x| Box::new(x) as Box<dyn Any + Send>)
        });

        let (sender, receiver) = oneshot::channel();
        let sync_closure = BridgeClosure { closure, sender };
        let operation = Operation::Transaction(OperationTransaction::Bridge(sync_closure));

        self.connection
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        let ret = receiver
            .await
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        // This cannot fail as the type system assures us that the return type of `callback` is T
        ret.map(|x| *x.downcast().expect("Downcast failed?"))
    }
}

impl Deref for Bond<'_> {
    type Target = Tether;

    fn deref(&self) -> &Self::Target {
        self.tether
    }
}

impl Drop for Bond<'_> {
    fn drop(&mut self) {
        _ = self
            .connection
            .send(Operation::Transaction(OperationTransaction::RollbackAbort));
    }
}

impl RunTransaction for Tether {
    fn tether(&self) -> &Tether {
        self
    }

    #[allow(clippy::manual_async_fn)]
    fn run_tx<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, anyhow::Error>,
    {
        async move {
            self.tx(closure)
                .await
                .context("Could not start transaction for tether")
        }
    }

    async fn run_tx_sync<T, F>(&mut self, closure: F) -> anyhow::Result<T>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> StashResult<T> + Send + 'static,
        T: Send + 'static,
    {
        self.sync_tx_returning(closure)
            .await
            .context("Could not start sync transaction for tether")
    }
}

/// This trait should only be used in functions that have to create and commit several
/// transactions.
/// It exists so that you can pass either a `&mut Tether` or a `&mut WriterGuard`.
pub trait RunTransaction {
    /// Get the tether instance that powers the transaction for read only queries.
    fn tether(&self) -> &Tether;

    /// Creates a transaction and run the given `closure`.
    fn run_tx<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, anyhow::Error>;

    fn run_tx_sync<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>> + Send
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> StashResult<T> + Send + 'static,
        T: Send + 'static;
}

impl<RT: RunTransaction> RunTransaction for &mut RT {
    fn tether(&self) -> &Tether {
        RT::tether(self)
    }

    #[allow(clippy::manual_async_fn)]
    fn run_tx<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, anyhow::Error>,
    {
        RT::run_tx(self, closure)
    }

    #[allow(clippy::manual_async_fn)]
    fn run_tx_sync<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>> + Send
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> StashResult<T> + Send + 'static,
        T: Send + 'static,
    {
        RT::run_tx_sync(self, closure)
    }
}

/// This encapsulates the logic of handling [`TetherOperation`]s.
/// An actor owning a queue in `Tether` should create this. This should be cleaned up when that
/// `Tether` gets dropped.
struct TetheredWorkerStateMachine<'a> {
    /// The transaction that might or might not be active
    transaction: Option<Transaction<'a>>,
    /// The sender we use to communicate with the main worker thread.
    connection: &'a Connection,
    state: State,
    watcher: &'a Watcher,
    was_interrupted: bool,
}

impl<'a> TetheredWorkerStateMachine<'a> {
    /// Handles a database operation.
    ///
    /// This function processes a database operation that the tethered worker
    /// has received from its connection-specific queue, executing the necessary
    /// logic against the database connection, and returning the result to the
    /// original caller. It is the core logic of the tethered worker thread, and
    /// is responsible for managing the connection and transaction state, and
    /// executing the queries.
    ///
    fn handle_operation(&mut self, operation: Operation, pool: &StashConnectionPool) -> bool {
        // If we were interrupted during a transaction, this value will be true.
        let mut should_quit = false;
        if self.was_interrupted {
            self.was_interrupted = false;
            // Any other operation that happens during the transaction needs to be notified
            // that the execution was interrupted.
            match operation {
                Operation::Transaction(op) => match op {
                    OperationTransaction::Commit(_, s) | OperationTransaction::Rollback(s) => {
                        let _ = s.send(Err(StashError::interrupted()));
                    }
                    OperationTransaction::Start(_, _) => {
                        // Starting a new transaction after an interrupt is fine since we
                        // wait until resume was called.
                        self.handle_transaction(op, pool);
                    }
                    OperationTransaction::StartSync(o, _) | OperationTransaction::Bridge(o) => {
                        let _ = o.sender.send(Err(StashError::interrupted()));
                    }
                    OperationTransaction::RollbackAbort => {
                        //nothing to do
                    }
                },
                Operation::Execution(op) => match op {
                    OperationExec::Instruct(o) => {
                        let _ = o.sender.send(Err(StashError::interrupted()));
                    }
                    OperationExec::Batch(o) => {
                        let _ = o.sender.send(Err(StashError::interrupted()));
                    }
                    OperationExec::Query(o) => {
                        let _ = o.sender.send(Err(StashError::interrupted()));
                    }
                    OperationExec::Sync(o) => {
                        let _ = o.sender.send(Err(StashError::interrupted()));
                    }
                },
                Operation::Interrupt => {
                    // do nothing.
                }
                Operation::Quit => {
                    should_quit = true;
                }
                Operation::ReturnToPool => {
                    self.handle_close();
                }
            };
            return should_quit;
        }

        match operation {
            Operation::Transaction(operation) => {
                self.handle_transaction(operation, pool);
            }
            Operation::Execution(operation) => {
                self.handle_exec(operation);
            }
            Operation::Interrupt => {
                self.handle_interrupt();
            }
            Operation::Quit => {
                should_quit = true;
            }
            Operation::ReturnToPool => {
                self.handle_close();
            }
        }

        should_quit
    }

    fn handle_interrupt(&mut self) {
        self.was_interrupted = self.transaction.is_some();
        // Rollback any active transactions.
        self.transaction = None;
    }

    fn handle_transaction(&mut self, operation: OperationTransaction, pool: &StashConnectionPool) {
        match operation {
            OperationTransaction::Start(policy, send_back) => {
                // Check whether we can start a new transaction or wait until we are allowed to.
                pool.check_interrupted_or_wait_resume();
                // In theory this should be impossible since we require a `&mut Tether` to start a
                // transaction
                assert!(self.transaction.is_none(), "Started transaction twice");
                match self.start_transaction(policy) {
                    Ok(transaction) => {
                        self.transaction = Some(transaction);
                        _ = send_back.send(Ok(()));
                    }
                    Err(error) => {
                        _ = send_back.send(Err(StashError::ExecutionError(error)));
                    }
                };
            }
            OperationTransaction::StartSync(BridgeClosure { closure, sender }, policy) => {
                // Check whether we can start a new transaction or wait until we are allowed to.
                pool.check_interrupted_or_wait_resume();
                let res = self.handle_start_sync(closure, policy);
                _ = sender.send(res);
            }

            OperationTransaction::Commit(policy, send_back) => {
                match self
                    .transaction
                    .take()
                    .map(|tx| self.commit_transaction(tx, policy))
                {
                    Some(Ok(())) => {
                        trace!("Commited transaction");
                        _ = send_back.send(Ok(()));
                    }
                    Some(Err(e)) => {
                        error!("Error when committing a transaction: {e:?}");
                        _ = send_back.send(Err(StashError::TransactionError(e)));
                    }
                    None => {
                        let err = anyhow!("Commit with no transaction open!?");
                        _ = send_back.send(Err(StashError::Critical(err)));
                    }
                }
            }
            OperationTransaction::Rollback(send_back) => {
                match self.transaction.take().map(|tx| tx.rollback()) {
                    Some(Ok(())) => {
                        debug!("Rolled back transaction");
                        _ = send_back.send(Ok(()));
                    }
                    Some(Err(e)) => {
                        error!("Error when rolling back a transaction: {e:?}");
                        _ = send_back.send(Err(StashError::TransactionError(e)));
                    }
                    None => {
                        let err = anyhow!("Rollback with no transaction open!?");
                        _ = send_back.send(Err(StashError::Critical(err)));
                    }
                }
            }
            OperationTransaction::RollbackAbort => {
                match self.transaction.take().map(|tx| tx.rollback()) {
                    Some(Ok(())) => {
                        debug!("Aborted transaction")
                    }
                    Some(Err(e)) => {
                        error!("Error when aborting a transaction (Bond drop): {e:?}");
                    }
                    None => {
                        error!("Critical error: RollbackAbort with no transaction open!?");
                    }
                }
            }
            OperationTransaction::Bridge(sync) => {
                let Some(tx) = &self.transaction else {
                    let e = anyhow!(
                        "Critical error: OperationTransaction::Bridge with no transaction open!?"
                    );
                    let _ = sync.sender.send(Err(e.into()));
                    return;
                };

                let res = (sync.closure)(tx);
                let _ = sync.sender.send(res);
            }
        }
    }

    fn start_transaction(
        &mut self,
        transaction_tracking_policy: TransactionTrackingPolicy,
    ) -> Result<Transaction<'a>, SqliteError> {
        if transaction_tracking_policy == TransactionTrackingPolicy::Tracking
            && let Err(e) = self
                .state
                .sync_tables(self.watcher)
                .execute(self.connection)
        {
            error!("Failed to sync tables: {e:?}");
            return Err(e);
        }
        // We call new_unchecked() here because new() requires a mutable borrow.
        // Being unchecked does not matter, as we perform the necessary checks
        // ourselves.
        Transaction::new_unchecked(
            self.connection,
            // This is not well-documented, but is significant. The behaviour mode of
            // the transaction affects when a lock is acquired - this part is obvious
            // and IS documented. NotifyRollbackTransactionFor reference:
            //
            //  - https://docs.rs/rusqlite/0.31.0/rusqlite/enum.TransactionBehavior.html
            //
            // A summary of the behaviour:
            //
            //  - DEFERRED means that the transaction does not actually start until the
            //    database is first accessed.
            //  - IMMEDIATE cause the database connection to start a new write
            //    immediately, without waiting for a writes statement.
            //  - EXCLUSIVE prevents other database connections from reading the
            //    database while the transaction is underway.
            //
            // So far, so good. The implication is that we can leave this to DEFERRED
            // (the default) and it will establish a higher level of locking as and when
            // needed. This is how things are documented in SQLite and rusqlite.
            //
            // However, what is not mentioned (and could be considered a bug? or at
            // least unexpected behaviour) is that if a transaction is started in
            // DEFERRED mode and then performs a read query before a write query, then
            // when the lock is upgraded the busy handler will not be triggered. This
            // then leads to concurrent operations being rejected with a "database is
            // locked" message, which does not happen under other circumstances.
            //
            // To state that again so that it's very clear: the busy timeout will be
            // respected as documented and expected for all instances where there are
            // multiple concurrent connections, transactions, queries, etc. and handle
            // them just fine, BUT it will have NO EFFECT if there is a read query
            // before a write query in a transaction.
            //
            // In order to work around this, we start the transaction in IMMEDIATE mode,
            // which registers our intent to write. Even if we don't actually end up
            // writing (and it is entirely valid to have transactions that only read),
            // this is necessary in order to have the busy timeout respected, and other
            // concurrent operations handled gracefully. This is why this appears to be
            // a bug, or at least behaviour that is undesirable and not in keeping with
            // the generally-described behaviour of these features.
            TransactionBehavior::Immediate,
        )
    }

    fn commit_transaction(
        &mut self,
        transaction: Transaction<'_>,
        transaction_tracking_policy: TransactionTrackingPolicy,
    ) -> Result<(), rusqlite::Error> {
        transaction.commit()?;
        if transaction_tracking_policy == TransactionTrackingPolicy::Tracking {
            self.state
                .publish_changes(self.watcher)
                .execute(self.connection)
                .inspect_err(|e| error!("Failed to report tracked changes: {e:?}"))?;
        }
        Ok(())
    }

    fn handle_exec(&mut self, operation: OperationExec) {
        let connection: &Connection = match self.transaction {
            Some(ref tx) => tx,
            None => self.connection,
        };

        match operation {
            OperationExec::Instruct(instruction) => {
                let res = instruction.run(connection);
                let _ = instruction.sender.send(res);
            }
            OperationExec::Batch(batch) => {
                let res = batch.run(connection);
                let _ = batch.sender.send(res);
            }
            OperationExec::Query(query) => {
                query.run_and_send(connection);
            }
            OperationExec::Sync(sync) => {
                let res = (sync.closure)(connection);
                let _ = sync.sender.send(res);
            }
        }
    }

    fn handle_close(&mut self) {
        let Some(transaction) = self.transaction.take() else {
            // No transaction happening, we can just exit the thread
            return;
        };

        if transaction.rollback().is_err() {
            error!("Failed to roll back transaction upon connection closure");
        }
    }

    fn handle_start_sync(
        &mut self,
        closure: Box<dyn FnOnce(&Transaction) -> SyncClosureRetTy + Send>,
        policy: TransactionTrackingPolicy,
    ) -> StashResult<Box<dyn Any + Send>> {
        // In theory this should be impossible since we require a `&mut Tether` to start a
        // transaction
        assert!(self.transaction.is_none(), "Started transaction twice");

        let tx = self
            .start_transaction(policy)
            .map_err(StashError::ExecutionError)?;

        match closure(&tx) {
            Err(user_err) => {
                tx.rollback().with_context(|| format!("Rollback error occurred when rolling back the transaction after this error: {user_err:?}"))?;
                Err(user_err)
            }
            Ok(e) => {
                self.commit_transaction(tx, policy)
                    .map_err(StashError::TransactionError)?;
                Ok(e)
            }
        }
    }
}

/// Value record struct used to generate the `DbRecord` glue code.
#[derive(Debug, Clone, PartialEq)]
struct ValueRecord<V: Clone + Debug + FromSql + ToSql + Send + Sync + PartialEq + 'static> {
    value: V,
}

impl<V: Clone + Debug + FromSql + ToSql + Send + Sync + PartialEq + 'static> DbRecord
    for ValueRecord<V>
{
    fn field_values(&self) -> impl Iterator<Item = &dyn ToSql> + '_ {
        [&self.value as &dyn ToSql].into_iter()
    }

    fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, ConversionError> {
        let value = row.get(0)?;
        Ok(Self { value })
    }
}
