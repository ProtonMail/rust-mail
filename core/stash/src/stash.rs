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

use crate::orm::{from_rows, perform_load, ConversionError, DbRecord, DbRecords, Model};
use anyhow::{anyhow, Context};
use core::fmt;
use core::fmt::Debug;
use core::future::Future;
use core::mem;
use core::ops::Deref;
use core::time::Duration;
use flume::{unbounded, Receiver as QueueReceiver, Sender as QueueSender};
use indoc::formatdoc;
use parking_lot::Mutex;
use r2d2::{Error as PoolError, ManageConnection, Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::hooks::Action;
use rusqlite::types::FromSql;
use rusqlite::{Connection, Error as SqliteError, Rows, ToSql, Transaction, TransactionBehavior};
use sqlite_watcher::connection::SqlConnectionAsync;
use sqlite_watcher::connection::SqlExecutorAsync;
use sqlite_watcher::connection::SqlTransactionAsync;
use sqlite_watcher::connection::State;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use sqlite_watcher::watcher::TableObserver;
use sqlite_watcher::watcher::Watcher;
use stash_macros::DbRecord;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender as StdSender_;
use std::sync::{mpsc, Arc};
use std::time::Instant;
use thiserror::Error;
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tokio::task::spawn_blocking;
use tracing::{debug, error, info, trace, warn};
// Used to resolve undeclared crate of module `stash` from DbRecord proc marco
use crate as stash;
use crate::registry::{StashRegistry, REGISTRY};

type StdSender<T> = flume::Sender<T>;
/// Set a timeout for a specified amount of time when a table is locked. This
/// defaults to 5,000 milliseconds in the underlying libraries.
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

/// The maximum number of simultaneous connections allowed to the database. This
/// defaults to 10.
// TODO: Test perf of lower values.
const MAX_CONNECTIONS: u32 = 100;

/// A type alias for a field convertor function.
type Converter = Box<dyn Fn(Rows<'_>) -> Result<DbRecords, ConversionError> + Send>;

/// A dual-nature connection wrapper.
///
/// This enum allows transparent handling of a connection, whether or not a
/// transaction is currently active. It is used only for representation of types
/// owned elsewhere, hence wraps references and borrows those instances.
///
/// It implements [`Deref`] so that it is essentially invisible to the caller.
///
enum AgnosticConnection<'tx> {
    NotTransaction(&'tx PooledConnection<SqliteConnectionManager>),
    Transaction(&'tx Transaction<'tx>),
}

impl Deref for AgnosticConnection<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        match *self {
            Self::NotTransaction(connection) => connection,
            Self::Transaction(transaction) => transaction,
        }
    }
}

/// The types of database operation that can be performed by the main worker.
///
/// A minimal command interface is provided, to issue instructions to the
/// background worker via MPSC queue. The worker will process the commands and
/// return the response via a oneshot channel. It is important to note that all
/// the messages sent need to be [`Send`] and [`Sync`], as they will be passed
/// between threads.
///
/// # See also
///
/// * [`Command`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`Query`]
/// * [`Subscription`]
/// * [`Worker`]
///
enum StashOperation {
    /// Notify a transaction was commited.
    NotifyCommitTransaction(u64),

    /// Notify a transaction was rolled back.
    NotifyRollbackTransaction(u64),

    /// Notify a new transaction has started.
    NotifyStartTransaction(u64),

    /// Publishes a notification of changes made to the database to all
    /// subscribers.
    Publish(Notification),

    /// Subscribes to notifications of changes made to the database.
    Subscribe(Subscription),
}

/// These are all the operations allowed on a tether.
enum TetherOperation {
    /// Only the operations related to a transaction.
    Transaction(OperationTransaction),
    /// Only the operations related to execution
    Execution(OperationExec),
}

#[derive(Debug)]
/// Only the operations related to a transaction.
enum OperationTransaction {
    /// Starts a new transaction.
    Start(OneshotSender<Result<(), StashError>>),

    /// Commits a transaction, i.e. finalises it.
    Commit(OneshotSender<Result<(), StashError>>),

    /// Rolls back a transaction, i.e. abandons it.
    Rollback(OneshotSender<Result<(), StashError>>),

    /// Rollbacks a transaction too.
    /// This one is meant to be called in Bond's drop glue. That's why it doesn't have a sender.
    /// Same semantics as Rollback.
    RollbackAbort,
}

/// This trait was designed for batched queries to efficiently create the queries just by borrowing
/// data and execute it in the actual db connection thread.
/// This allows us to efficiently convert the data into a query, skipping having to send thousands of
/// `Vec<Box<dyn ToSql>>`
///
/// This trait is automatically implemented for all `[Model]`s, so that it can be used with any
/// smart pointer:
/// - Vec<M>
/// - Arc<[M]>
/// - Box<[M]>
///
/// where M: Model.
///
/// It's theoretically possible to implement this on other types, like API types directly.
///
/// This trait is meant to be used as a trait object.
/// You might notice the `RetId` part of the name, this is because the execute returns the inserted
/// IDs. This could be extended in the future to return arbitrary data, or no data at all.
pub trait BatchQueryRetId: Send {
    fn query(&self) -> String;
    /// This returns a `Vec<u64>`, where the u64 is of the id of the model.  
    fn execute(&self, stmt: &'_ mut rusqlite::Statement<'_>) -> Result<Vec<u64>, StashError>;
}

/// Make sure that it's object safe
#[allow(dead_code)]
fn _f(_: &dyn BatchQueryRetId) {}

// TODO: I'm not sure if this impl is strictly needed.
impl<T: Deref<Target = [M]> + Send, M: Model + Send> BatchQueryRetId for T {
    fn query(&self) -> String {
        <Self as Deref>::deref(self).query()
    }

    fn execute(&self, stmt: &'_ mut rusqlite::Statement<'_>) -> Result<Vec<u64>, StashError> {
        <Self as Deref>::deref(self).execute(stmt)
    }
}

impl<M: Model + Send> BatchQueryRetId for [M] {
    fn query(&self) -> String {
        let field_names = M::field_names_without_id();
        format!(
            "INSERT INTO {} ({}) VALUES ({})
            RETURNING {} AS id",
            M::table_name(),
            field_names.join(","),
            crate::utils::placeholders(field_names.len()),
            M::id_field_name(),
        )
    }

    fn execute(&self, stmt: &'_ mut rusqlite::Statement<'_>) -> Result<Vec<u64>, StashError> {
        let mut out = vec![];
        for i in self {
            // PERF: This could be optimized in a big way.
            let params = i.field_values_without_id();
            let id = stmt
                .query_row(&*prepare_params(&params), |row| row.get(0))
                .map_err(StashError::ExecutionError)?;
            out.push(id);
        }
        Ok(out)
    }
}

/// This gets constructed from [`Tether::batch_write`] in order to perform many inserts more
/// efficiently.
struct BatchedWrite {
    params: Box<dyn BatchQueryRetId>,

    /// The communication channel used to send the result of the operation back
    /// to the caller.
    sender: OneshotSender<Result<Vec<u64>, StashError>>,
}

impl BatchedWrite {
    /// Prepares and executes a query, and returns the ids.
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<Vec<u64>, StashError> {
        let mut statement = connection
            .prepare(&self.params.query())
            .map_err(StashError::PreparationError)?;

        self.params.execute(&mut statement)
    }
}

enum OperationExec {
    /// A query to be executed, where no results are expected. This is usually
    /// a write query, or a command, but differentiation is up to the caller and
    /// not enforced.
    Instruct(Instruction),

    /// A mass insert function that returns the ids of the records inserted.
    BatchedInsertReturningIds(BatchedWrite),

    /// A query to be executed, where results are expected. This is typically a
    /// read query, but could be any query where results are expected, such as
    /// an `INSERT` query that returns the ID of the inserted row.
    Query(Query),
}

/// Error type for the [`Stash`] module.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StashError {
    /// There was a problem with deserialising the query results. This means
    /// that serde failed to convert the query results into the desired type,
    /// which could be due to a mismatch between the query results and the
    /// expected type.
    #[error("Query results deserialization error: {0}")]
    DeserializationError(#[from] ConversionError),

    /// A problem was experienced when attempting to downcast a boxed trait
    /// object. This should never happen in practice.
    #[error("Downcast error")]
    DowncastError,

    /// There was a problem with statement execution. Note that this refers to
    /// executing a prepared statement, e.g. actually running a query, and not
    /// the process of preparing the statement/query.
    #[error("Statement execution error: {0}")]
    ExecutionError(SqliteError),

    /// The primary key ID was received as [`None`] in a location where it was
    /// expected to be [`Some`]. This implies that either the records were not
    /// previously saved, and were expected to be, or that there is some kind of
    /// misconfiguration relating to primary keys, such as `AUTOINCREMENT` not
    /// being set when it should be.
    #[error("ID not set")]
    IdNotSet,

    /// There is a row ID, but no primary ID field value — or, no row ID, but
    /// the primary ID field is set when configured as auto-incrementing.
    /// Neither of these situations should ever happen in normal practice, and
    /// if they do, it means some manual process has taken place that has
    /// resulted in an invalid state.
    #[error("Row ID and ID field are in an invalid state")]
    InvalidIdState,

    /// The row ID was missing from the query results. This should never happen
    /// in practice as the only places that rely on it are responsible for
    /// specifying it in the queries that get run. Manual queries may not
    /// specify it, but that doesn't matter.
    #[error("Missing row ID")]
    MissingRowId,

    /// An operation requiring a transaction was attempted, such as a commit or
    /// rollback, but no active transaction was found.
    #[error("No active transaction")]
    NoActiveTransaction,

    /// There was a problem with statement preparation. Note that this refers to
    /// preparing a statement from a query and parameters, prior to execution.
    #[error("Statement preparation error: {0}")]
    PreparationError(SqliteError),

    /// No row ID was returned after saving a record. This should never happen.
    #[error("No row ID returned after saving record")]
    NoRowIdReturned,

    /// No [`Stash`] is available to use. This usually implies that functions
    /// are being called against a [`Model`] instance without setting the
    /// `stash` property first.
    #[error("No Stash available to use")]
    NoStashAvailable,

    /// No rows were updated upon saving a record. This can happen if the record
    /// data hasn't changed, in which case it's not an error — but in other
    /// situations, it would signify a problem.
    #[error("No rows updated upon saving record")]
    NoRowsUpdated,

    /// There was a problem with receiving from a oneshot channel. This should
    /// never happen in practice. Note that this only indicates a problem with
    /// receiving, and not with sending — it is not possible to return an error
    /// anywhere if sending fails, and so that is simply logged.
    #[error("Oneshot channel error: Receiving failed: {0}")]
    OneShotError(String),

    /// There was a problem with sending to the background worker's queue. This
    /// should never happen in practice. Note that this only indicates a problem
    /// with sending, and not with receiving — it is not possible to report an
    /// error with receiving, as any error would result in the queue handle
    /// being dropped, which cannot be detected.
    #[error("Queue error: Sending failed: {0}")]
    QueueError(String),

    /// There was a problem with subscriptions. For some reason the subscription
    /// has ended up in the wrong place. This should never happen in practice.
    #[error("Subscription error")]
    SubscriptionError,

    /// There was a problem with subscriptions. For some reason the subscription
    /// has ended up in the wrong place. This should never happen in practice.
    #[error("Watcher error: `{0}`")]
    WatcherError(String),

    /// There was a problem establishing a tether to the [`Stash`], which could
    /// be to do with creating the actual stash, or connecting to the service.
    #[error("Stash tether error: {0}")]
    TetherError(#[from] PoolError),

    /// An attempt was made to start a transaction, but one is already active.
    #[error("Transaction already started")]
    TransactionAlreadyStarted,

    /// An attempt was made to use transaction operations without a [`Tether`].
    #[error("Transaction command without Tether")]
    TransactionCommandWithoutTether,

    /// There was a problem with a transaction.
    #[error("Error starting the transaction")]
    TransactionStartError,

    /// There was a problem with a transaction.
    #[error("Transaction error: {0}")]
    TransactionError(SqliteError),

    /// Custom error that can be returned when an error occurs during
    /// implementations of `on_save()` or `on_load()` for [`Model`].
    #[error("Custom: {0}")]
    Custom(String),

    /// Critical error that cannot be recovered from.
    #[error("Critical error: {0}")]
    Critical(#[from] anyhow::Error),
}

/// An operation to be executed by the worker, which does not return any data.
///
/// This is used for operations such as `INSERT`, `UPDATE`, and `DELETE`, where
/// the result is the number of rows affected, along with other similar
/// commands.
///
struct Instruction {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    sender: OneshotSender<Result<usize, StashError>>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl Instruction {
    /// Prepares and executes a query, and returns the number of affected rows.
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<usize, StashError> {
        let mut statement = connection
            .prepare(&self.query)
            .map_err(StashError::PreparationError)?;
        let affected = statement
            .execute(&*prepare_params(&self.params))
            .map_err(StashError::ExecutionError)?;
        // I'm not sure if we should do this.
        // TODO : Put this behind a feature flag (next MR)
        if let Some(query) = statement.expanded_sql() {
            trace!("Query: {query}");
        }
        Ok(affected)
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

/// An operation to be executed by the worker, which returns data.
///
/// This is used for operations such as `SELECT`, where the result is a set of
/// rows of data. Notably, the deserialisation function is also passed, so that
/// the results can be converted into the desired type. This is because the
/// [`Rows`] type returned by the [`rusqlite`] library is not thread-safe.
///
struct Query {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    sender: OneshotSender<Result<DbRecords, StashError>>,

    /// The deserialisation function to use to convert the query results into
    /// the desired type. This is necessary because the [`Rows`] type returned
    /// by the [`rusqlite`] library is not thread-safe.
    converter: Converter,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl Query {
    /// Prepares and executes a query, and returns any rows of data emitted.
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<DbRecords, StashError> {
        let mut statement = connection
            .prepare(&self.query)
            .map_err(StashError::PreparationError)?;
        let rows: Result<DbRecords, ConversionError> = (self.converter)(
            statement
                .query(&*prepare_params(&self.params))
                .map_err(StashError::ExecutionError)?,
        );
        if let Some(query) = statement.expanded_sql() {
            debug!("Query: {query}");
        }
        if let Ok(ref records) = rows {
            debug!("Rows: {}", records.0.len());
        }
        rows.map_err(StashError::DeserializationError)
    }
}

/// This is stash's database pool. Its main use is to create [`Tether`]s.
// Internally this spawns a task that handles all of the operations (See [`StashOperation`]).
#[derive(Clone)]
pub struct Stash {
    /// TODO: remove this field.
    pub(crate) handle: Arc<()>,

    /// The sender for the stash operations that goes to [`Worker`]
    queue: QueueSender<StashOperation>,

    /// The [`Watcher`] instance for the [`Stash`], which is used to monitor the
    /// database for changes and notify subscribers. This is used to provide
    /// real-time updates to any subscribers that have registered interest in
    /// changes to the database for given tables.
    watcher: Arc<Watcher>,

    /// The pool used for database connections.
    pool: Pool<SqliteConnectionManager>,
}

impl Debug for Stash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut r = f.debug_struct("Stash");
        _ = r.field("handle", &self.handle).field("queue", &self.queue);

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
    /// # Parameters
    ///
    /// * `path` - The path to the SQLite database file. If `None`, an in-memory
    ///            database is created.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem creating
    /// the database or connection pool.
    ///
    pub fn new(path: Option<&Path>) -> Result<Self, StashError> {
        let (sender, receiver) = unbounded();
        let stash = Self {
            pool: Self::make_pool(path),
            handle: Arc::new(()),
            queue: sender.clone(),
            watcher: Watcher::new().map_err(|e| StashError::WatcherError(e.to_string()))?,
        };
        Worker::start(receiver)?;
        Ok(stash)
    }

    /// Create a sqlite pool.
    /// This is infaliable, if it cannot open the file it will fail later on when we try to
    /// connect.
    #[allow(clippy::missing_panics_doc)] // This can only happen if we misconfigure the pool.
    fn make_pool(path: Option<&Path>) -> Pool<SqliteConnectionManager> {
        #[allow(clippy::single_match_else)]
        match path {
            Some(p) => debug!("New Stash with file: {:?}", p),
            None => debug!("New Stash with in-memory database"),
        }
        let manager = path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );
        Pool::builder()
            .max_size(MAX_CONNECTIONS)
            .build(manager)
            .expect("Could not open that many connections")
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
    #[must_use]
    pub fn connection(&self) -> Tether {
        Tether::new(self.clone())
    }

    /// Factory method that uses the registry.
    ///
    /// This method is used to get a [`Stash`] instance from the registry. If
    /// the instance does not exist, it is created and added to the registry.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to the SQLite database file. If `None`, an in-memory
    ///            database is created.
    ///
    /// # Errors
    ///
    /// If there is a problem creating the database or connection pool, an error
    /// will be returned.
    ///
    pub fn get_instance(path: &Path) -> Result<Self, StashError> {
        let global = REGISTRY.get_or_init(|| Mutex::new(StashRegistry::new()));

        let mut registry = global.lock();

        // Periodically clean up dead entries
        if fastrand::bool() {
            registry.cleanup();
        }
        registry.get_or_create(path.to_path_buf())
    }

    /// Subscribes to notifications of changes to the database.
    ///
    /// This function subscribes to notifications of changes to the database. It
    /// returns a queue receiver which will be sent [`Notification`] instances
    /// containing information about the changes made.
    ///
    /// At present this is a wide-spectrum subscription, and will receive
    /// notifications for all changes made to the database. In future it will be
    /// possible to filter this.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    /// * [`StashError::OneShotError`]
    /// * [`StashError::QueueError`]
    /// * [`StashError::SubscriptionError`]
    ///
    /// # See alse
    ///
    /// * [`Notification`]
    ///
    pub async fn subscribe(&self) -> Result<QueueReceiver<Notification>, StashError> {
        self.subscribe_internal(None).await
    }

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

    /// Internal helper method to handle database change subscriptions.
    ///
    /// # Parameters
    ///
    /// * `table` - Optional table name to subscribe to. If None, subscribes to all tables.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    /// * [`StashError::OneShotError`]
    /// * [`StashError::QueueError`]
    /// * [`StashError::SubscriptionError`]
    ///
    async fn subscribe_internal(
        &self,
        table: Option<String>,
    ) -> Result<QueueReceiver<Notification>, StashError> {
        let (that_end, this_end) = oneshot::channel();
        let (sender, receiver) = unbounded::<Notification>();
        let operation = StashOperation::Subscribe(Subscription {
            channel: that_end,
            queue: sender,
            table,
        });
        self.queue
            .send(operation)
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))??;
        Ok(receiver)
    }
}

/// A handle to a database connection watcher.
#[derive(Debug)]
#[non_exhaustive]
pub struct WatcherHandle {
    /// The receiver for the notifications.
    pub receiver: QueueReceiver<()>,
    /// The handle to stop the watcher.
    pub handle: DropRemoveTableObserverHandle,
}

/// A subscription operation to be executed by the worker.
///
/// This is used for subscribing to [`Notification`]s, such as database change
/// events.
struct Subscription {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: OneshotSender<Result<(), StashError>>,

    /// The queue to which [`Notification`]s will be sent. Note that this is
    /// for *redistributed* notifications — i.e. after the central worker has
    /// received them from the database, it will then send them to all
    /// subscribers, with this being a subscriber-specific queue.
    queue: QueueSender<Notification>,

    /// The table to subscribe to. If [`None`], all tables are subscribed to.
    table: Option<String>,
}

impl Subscription {
    fn send(self, result: Result<(), StashError>) {
        if self.channel.send(result).is_err() {
            // This means that the receiver has dropped.
            error!("Oneshot error: Failed sending result back to caller");
        }
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
    /// This is the channel that sends [`TetherOperation`]s to the inner thread.
    /// This was changed to a std channel since flume seems to hang on sync <-> async under some
    /// circumstances.
    sender: StdSender<TetherOperation>,

    watcher: Arc<Watcher>,

    /// State needed for the connection to be updated on transaction start and
    /// published at the end.
    state: Option<State>,
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
    /// # Parameters
    ///
    /// * `query`  - The query to execute.
    /// * `params` - The parameters to pass to the query.
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
        let operation = TetherOperation::Execution(OperationExec::Instruct(Instruction {
            sender,
            params,
            query: query.into(),
        }));
        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")
            .map_err(Into::into)
    }

    /// Sends a batch of `INSERT`.
    /// In order to get `params`, you just need to Box any slice (or smart ptr that derefs into one
    /// like Vec, Box, Arc...) into it:
    ///
    /// ```rs
    ///    pub async fn batch_write_arc(&self, params: Arc<[impl Model]>) -> Result<Vec<u64>, StashError> {
    ///        &self,
    ///        params: Vec<impl Model>,
    ///    ) -> Result<Vec<u64>, StashError> {
    ///        let b: Box<dyn GetParams> = Box::new(params);
    ///        self.batch_write(b).await
    ///    }
    ///
    ///    pub async fn batch_write_vec(&self, params: Vec<impl Model>) -> Result<Vec<u64>, StashError> {
    ///        &self,
    ///        params: Vec<impl Model>,
    ///    ) -> Result<Vec<u64>, StashError> {
    ///        let b: Box<dyn GetParams> = Box::new(params);
    ///        self.batch_write(b).await
    ///    }
    /// ```
    pub async fn batch_write(
        &self,
        params: Box<dyn BatchQueryRetId>,
    ) -> Result<Vec<u64>, StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation =
            TetherOperation::Execution(OperationExec::BatchedInsertReturningIds(BatchedWrite {
                sender,
                params,
            }));
        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")
    }

    /// Loads a record from the database by ID.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID, as an instance of the specified type `T`, where `T` is any concrete
    /// type implementing the [`Model`] trait.
    ///
    /// For full usage details, see [`Model::load()`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID of the record to load.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    pub async fn load<T, I>(&self, id: I) -> Result<Option<T>, StashError>
    where
        T: Model,
        I: ToSql + Send + 'static,
    {
        perform_load(id, self).await
    }

    /// Runs a query and returns any rows of data emitted.
    ///
    /// This function prepares a query and executes it on the database, and
    /// returns the resulting rows of data as a collection of instances of the
    /// specified `T` type, where `T` is any concrete type implementing the
    /// [`DbRecord`] trait. The requirement to formalise the return type
    /// streamlines the process of handling the results.
    ///
    /// Note that the [`params!`](crate::utils::params) macro is available to
    /// help shorten the syntax for passing in the query parameters.
    ///
    /// # Read vs write
    ///
    /// Although this function is *designed* for read queries, this is implied
    /// and a convenience, and it is entirely possible to use it for write
    /// queries as well. The number of rows returned will be zero for write
    /// queries. Any semantic difference between read and write queries is left
    /// to the caller to decide, and does not result in any difference in
    /// handling by this module. The [`rusqlite`] library will handle the
    /// distinction as necessary.
    ///
    /// # Deserialisation
    ///
    /// Note that it is possible to deserialise the results into other types
    /// too, and indeed serialise types into queries, but those use cases are
    /// unlikely to be needed, or at least not common, and so are not provided
    /// by this module. No interface is currently provided to achieve this.
    ///
    /// # Parameters
    ///
    /// * `query`  - The query to execute.
    /// * `params` - The parameters to pass to the query.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`DeserializationError`](StashError::DeserializationError) - Problem
    ///     converting from [`Rows`] to `T`.
    ///   - [`ExecutionError`](StashError::ExecutionError) - Problem executing
    ///     the query.
    ///   - [`OneShotError`](StashError::OneShotError) - Problem receiving data
    ///     back to the caller via the oneshot channel.
    ///   - [`PreparationError`](StashError::PreparationError) - Problem
    ///     preparing the query.
    ///   - [`QueueError`](StashError::QueueError) - Problem sending the
    ///     operation to the queue.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///
    /// # See also
    ///
    /// * [`Interface::execute()`]
    /// * [`params!`](crate::utils::params)
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn query<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String> + Send,
        T: DbRecord + Send + 'static,
        DbRecords: FromIterator<Box<T>>,
    {
        let (sender, receiver) = oneshot::channel();
        let query = Query {
            sender,
            converter: Box::new(converter::<T>),
            params,
            query: query.into(),
        };
        let operation = TetherOperation::Execution(OperationExec::Query(query));
        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;

        Ok(receiver
            .await
            .expect("Tether closed its channel with handles still open")?
            .into_iter()
            .map(|item| {
                // The type we receive back is described as Any so that it can pass through
                // the channel without introducing unnecessary type constraints, but is in
                // fact already known to be of type T, so we can downcast it safely.
                *item.downcast::<T>().unwrap()
            })
            .collect())
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
    ///         "SELECT number AS value FROM table",
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
    ///         "SELECT number AS value FROM table",
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
    ///
    /// This function is similar in nature to [`Interface::query_values()`] but
    /// it returns only one value.
    ///
    /// # Errors
    ///
    /// If no rows are returned, this function returns
    /// [`SqliteError::QueryReturnedNoRows`].
    ///
    /// # See also
    ///
    /// * [`Interface::query_values()`]
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
        let mut values = self.query_values::<Q, T>(query, params).await?;
        if values.is_empty() {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        }

        if values.len() > 1 {
            return Err(StashError::Custom("Query returned multiple rows".into()));
        }

        Ok(values.swap_remove(0))
    }

    /// Starts a new transaction.
    ///
    /// This function starts a new transaction. All queries executed within the transaction must be
    /// executed against the same connection, which is why a new transaction consumes the [`Tether`].
    ///
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
    /// # See also
    ///
    /// * [`Stash::connection()`]
    ///
    pub async fn transaction(&mut self) -> Result<Bond<'_>, StashError> {
        self.listen_for_changes().await?;
        self.quiet_transaction().await
    }

    /// The transaction will produce no any notifications.
    ///
    /// This method is used to start a transaction without listening for changes.
    /// It is needed for internal implementation of the watch mechanism and scrollers.
    ///
    /// # Errors
    ///
    /// see [`Tether::transaction()`]
    pub async fn quiet_transaction(&mut self) -> Result<Bond<'_>, StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation = TetherOperation::Transaction(OperationTransaction::Start(sender));

        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")?;

        Ok(Bond::new(self))
    }

    /// Listens for changes to the database.
    async fn listen_for_changes(&mut self) -> Result<(), StashError> {
        let Some(mut state) = self.state.take() else {
            tracing::error!(
                "No state found for Tether, something is very wrong with notification system"
            );
            return Err(StashError::Custom("No state found for Tether".into()));
        };
        let watcher = Arc::clone(&self.watcher);
        let result = state.sync_tables_async(self, &watcher).await;

        self.state = Some(state);

        result
    }

    /// Publishes changes to the database.
    async fn publish_changes(&mut self) -> Result<(), StashError> {
        let Some(mut state) = self.state.take() else {
            tracing::error!(
                "No state found for Tether, something is very wrong with notification system"
            );
            return Err(StashError::Custom("No state found for Tether".into()));
        };
        let watcher = Arc::clone(&self.watcher);
        let result = state.publish_changes_async(self, &watcher).await;

        self.state = Some(state);

        result
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
    /// # Parameters
    ///
    /// * `conn_handle` - The handle of the connection to use for the queries. A
    ///                   connection-specific worker in its own dedicated thread
    ///                   will be created and associated, storing this weak
    ///                   reference internally.
    /// * `pool`        - The SQLite connection pool to use for the queries.
    /// * `queue`       - The main operations queue, shared with the main
    ///                   worker and other tethered workers.
    /// * `stash`       - The associated [`Stash`] instance for the operations.
    ///
    fn new(stash: Stash) -> Self {
        let (tether_sender, tether_receiver) = flume::unbounded::<TetherOperation>();

        let pool = stash.pool.clone();
        let queue_clone = stash.queue.clone();
        // Spawn a thread to run the worker. This thread will execute the queries
        // sequentially, as they are received, on a persistent connection, and will
        // return the results to the original caller via oneshot channels.
        debug!("Spawning worker task...");
        _ = spawn_blocking(move || {
            debug!("Creating worker thread");
            // The first time an operation is received, we attempt to acquire a database
            // connection from the pool. This is done lazily so that creating tethers is sync.
            // Note that most of this logic could be avoided if we made tether cration async.

            #[allow(clippy::items_after_statements)]
            // This is scoped here so that we can't create an id from anywhere else.
            static TETHER_ID: AtomicU64 = AtomicU64::new(0);
            let id = TETHER_ID.fetch_add(1, Ordering::Relaxed);
            info!("Creating tether {id}");

            let queue_clone_2 = queue_clone.clone();
            let connection = || {
                let mut connection = pool
                    .get_and_subscribe(queue_clone_2, id)
                    .context("Could not connect to the database")?;
                Self::conn_configuration(&connection)
                    .context("Could not set connection configuration.")?;
                State::start_tracking(&mut *connection)
                    .context("Critical error: Failed to set watcher on the connection")?;
                debug!("Success connecting to db");
                Ok::<_, StashError>(connection)
            };

            let (first_operation, connection) = match (tether_receiver.recv(), connection()) {
                (Ok(op), Ok(con)) => (op, con),
                (Ok(op), Err(e)) => {
                    error!("Critical error creating worker {e}");
                    match op {
                        TetherOperation::Transaction(
                            OperationTransaction::Start(ch)
                            | OperationTransaction::Rollback(ch)
                            | OperationTransaction::Commit(ch),
                        ) => {
                            _ = ch.send(Err(e));
                        }
                        TetherOperation::Execution(OperationExec::Instruct(x)) => {
                            if x.sender.send(Err(e)).is_err() {
                                // This means that the receiver has dropped.
                                error!("Oneshot error: Failed sending result back to caller");
                            };
                        }
                        TetherOperation::Execution(OperationExec::Query(x)) => {
                            if x.sender.send(Err(e)).is_err() {
                                // This means that the receiver has dropped.
                                error!("Oneshot error: Failed sending result back to caller");
                            };
                        }
                        TetherOperation::Execution(OperationExec::BatchedInsertReturningIds(x)) => {
                            if x.sender.send(Err(e)).is_err() {
                                // This means that the receiver has dropped.
                                error!("Oneshot error: Failed sending result back to caller");
                            };
                        }
                        TetherOperation::Transaction(OperationTransaction::RollbackAbort) => {
                            unreachable!("This cannot happen at this point")
                        }
                    };
                    return;
                }
                (Err(_), _) => {
                    warn!("Tether dropped before sending anything");
                    return;
                }
            };

            let queue = InfallibleSenderAsync {
                sender: queue_clone,
                reason: "Failed to send NotifyStartTransaction operation to main queue.
This means that the main worker thread has closed with open handles to it. 
This cannot happen, the main worker thread is not supposed to close.",
            };

            debug!("Starting tether {id} worker");
            let mut sm = TetheredWorkerStateMachine {
                transaction: None,
                connection: &connection,
                id,
                queue,
                last_op: "None yet",
            };
            sm.handle_operation(first_operation);

            debug!("{id} Waiting for more...");
            while let Ok(operation) = tether_receiver.recv() {
                sm.handle_operation(operation);
                debug!("{id} Waiting for more...");
            }
            sm.handle_close();
        });

        Self {
            sender: tether_sender,
            watcher: stash.watcher.clone(),
            state: Some(State::new()),
        }
    }

    fn conn_configuration(
        connection: &PooledConnection<SqliteConnectionManager>,
    ) -> Result<(), SqliteError> {
        connection
        .execute_batch(&formatdoc!("
            PRAGMA journal_mode = WAL;         -- Better write-concurrency
            PRAGMA synchronous = NORMAL;       -- Perform fsync only at critical points
            PRAGMA wal_autocheckpoint = 1000;  -- Write WAL changes back every 1000 pages, approx. 1MB
            PRAGMA wal_checkpoint(TRUNCATE);   -- Free space by truncating WAL files from the last run
            PRAGMA busy_timeout = {};          -- Wait if the database is busy/locked
            PRAGMA foreign_keys = ON;          -- Enforce foreign key constraints
            PRAGMA temp_store = MEMORY;        -- Allows temporary storage for watcher
            PRAGMA recursive_triggers='ON';    -- Allows recursive triggers for watcher
        ", BUSY_TIMEOUT.as_millis()))
    }
}

impl Debug for Tether {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tether").finish_non_exhaustive()
    }
}

impl SqlExecutorAsync for Tether {
    type Error = StashError;
    #[allow(clippy::indexing_slicing)]
    #[allow(clippy::manual_async_fn)]
    fn sql_query_values(
        &mut self,
        query: &str,
    ) -> impl Future<Output = Result<Vec<usize>, Self::Error>> + Send {
        async {
            let query_parts = query.split(" FROM ").collect::<Vec<&str>>();
            if query_parts.len() != 2 {
                return Err(StashError::Custom(
                    "Invalid query format. Expected 'SELECT ... FROM ...'".into(),
                ));
            }
            let new_query = format!("{} as value FROM {}", query_parts[0], query_parts[1]);
            self.query_values::<_, usize>(new_query, vec![]).await
        }
    }

    #[allow(unused_results)]
    #[allow(clippy::manual_async_fn)]
    fn sql_execute(&mut self, query: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async {
            self.execute(query.to_owned(), vec![]).await?;
            Ok(())
        }
    }
}

impl SqlConnectionAsync for Tether {
    fn sql_transaction(
        &mut self,
    ) -> impl Future<Output = Result<impl SqlTransactionAsync<Error = Self::Error> + '_, Self::Error>>
           + Send {
        self.quiet_transaction()
    }
}

impl SqlExecutorAsync for Bond<'_> {
    type Error = StashError;
    fn sql_query_values(
        &mut self,
        query: &str,
    ) -> impl Future<Output = Result<Vec<usize>, Self::Error>> + Send {
        self.tether.sql_query_values(query)
    }

    fn sql_execute(&mut self, query: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.tether.sql_execute(query)
    }
}

impl SqlTransactionAsync for Bond<'_> {
    fn sql_commit_transaction(self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.commit_(false)
    }
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

    /// Commits a transaction.
    ///
    /// This function commits, i.e. finalises, an existing, active transaction.
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
    ///   - [`TransactionError`](StashError::ExecutionError) - Problem
    ///     committing the transaction.
    ///
    pub async fn commit(self) -> Result<(), StashError> {
        self.commit_(true).await
    }

    /// Do not notify watchers about a changes, use in par with `quiet_transaction`.
    ///
    /// This method is used to commit a transaction without publishing changes.
    /// It is needed for internal implementation of the watch mechanism and scrollers.
    ///
    /// # Errors
    ///
    /// see [`Bond::commit()`]
    ///
    pub async fn quiet_commit(self) -> Result<(), StashError> {
        self.commit_(false).await
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
    async fn commit_(self, publish_changes: bool) -> Result<(), StashError> {
        let (sender, receiver) = oneshot::channel();
        let operation = TetherOperation::Transaction(OperationTransaction::Commit(sender));
        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;

        if let Err(e) = receiver
            .await
            .expect("Tether closed its channel with handles still open")
        {
            error!("Commit error: {e}");
            self.rollback().await?;
            return Ok(());
        } else if publish_changes {
            debug!("Publishing changes after commit.");
            self.tether.publish_changes().await?;
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
    pub async fn rollback(self) -> Result<(), StashError> {
        let (sender, receiver) = oneshot::channel();

        let operation = TetherOperation::Transaction(OperationTransaction::Rollback(sender));
        self.sender
            .send(operation)
            .map_err(|_| anyhow!("The stash worker dropped"))?;
        receiver
            .await
            .expect("Tether closed its channel with handles still open")?;

        // Transaction rolled back, skip the drop logic
        mem::forget(self);

        Ok(())
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
        self.sender.send(TetherOperation::Transaction(
            OperationTransaction::RollbackAbort,
        ));
    }
}

struct InfallibleSenderAsync<T> {
    sender: QueueSender<T>,
    reason: &'static str,
}

impl<T> Debug for InfallibleSenderAsync<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_name = std::any::type_name::<T>();
        write!(f, "InfallibleSenderAsync<{type_name}>")
    }
}

impl<T: Send> InfallibleSenderAsync<T> {
    fn send(&self, msg: T) {
        self.sender.send(msg).expect(self.reason);
    }
}

/// This encapsulates the logic of handling [`TetherOperation`]s.
/// An actor owning a queue in `Tether` should create this. This should be cleaned up when that
/// `Tether` gets dropped.
struct TetheredWorkerStateMachine<'a> {
    /// The transaction that might or might not be active
    transaction: Option<Transaction<'a>>,
    /// The sender we use to communicate with the main worker thread.
    queue: InfallibleSenderAsync<StashOperation>,
    connection: &'a PooledConnection<SqliteConnectionManager>,
    /// This is a unique id used for notifications
    id: u64,
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
    /// # Parameters
    ///
    /// * `operation`   - The database operation to handle.
    /// * `connection`  - The database connection to use for the operation. This
    ///                   is used to run queries when there is no transaction
    ///                   currently active.
    /// * `transaction` - The active transaction, if any. Notably, ownership is
    ///                   taken and returned, to avoid borrowing issues in the
    ///                   main loop that calls this function.
    /// * `stash`       - The associated [`Stash`] instance for the operation.
    /// * `queue`       - The main operations queue for the central worker.
    ///
    fn handle_operation(&mut self, operation: TetherOperation) {
        match operation {
            TetherOperation::Transaction(operation) => {
                self.handle_transaction(operation);
            }
            TetherOperation::Execution(operation) => {
                self.handle_exec(operation);
            }
        }
    }
    fn handle_transaction(&mut self, operation: OperationTransaction) {
        match operation {
            OperationTransaction::Start(send_back) => {
                // In theory this should be impossible since we require a `&mut Tether` to start a
                // transaction
                assert!(self.transaction.is_none(), "Started transaction twice");
                match self.start_transaction() {
                    Ok(transaction) => {
                        self.transaction = Some(transaction);

                        // Notify the main worker that a transaction has started.
                        self.queue
                            .send(StashOperation::NotifyStartTransaction(self.id));
                        _ = send_back.send(Ok(()));
                    }
                    Err(error) => {
                        _ = send_back.send(Err(StashError::ExecutionError(error)));
                    }
                };
            }
            OperationTransaction::Commit(send_back) => {
                {
                    // Notify the main worker that the transaction has been committed
                    self.queue
                        .send(StashOperation::NotifyCommitTransaction(self.id));

                    match self.transaction.take().map(|tx| tx.commit()) {
                        Some(Ok(())) => {
                            trace!("Commited transaction");
                            _ = send_back.send(Ok(()));
                        }
                        Some(Err(e)) => {
                            error!("Error when committing a transaction: {e}");
                            _ = send_back.send(Err(StashError::TransactionError(e)));
                        }
                        None => {
                            error!("Critical error: Rollback with no transaction open!?");
                        }
                    }
                }
            }
            OperationTransaction::Rollback(send_back) => {
                // Notify the main worker that the transaction has been rolled back.
                self.queue
                    .send(StashOperation::NotifyRollbackTransaction(self.id));

                match self.transaction.take().map(|tx| tx.rollback()) {
                    Some(Ok(())) => {
                        debug!("Rolled back transaction");
                        _ = send_back.send(Ok(()));
                    }
                    Some(Err(e)) => {
                        error!("Error when rolling back a transaction: {e}");
                        _ = send_back.send(Err(StashError::TransactionError(e)));
                    }
                    None => {
                        error!("Critical error: Rollback with no transaction open!?");
                    }
                }
            }
            OperationTransaction::RollbackAbort => {
                // Notify the main worker that the transaction has been rolled back.
                self.queue
                    .send(StashOperation::NotifyRollbackTransaction(self.id));

                match self.transaction.take().map(|tx| tx.rollback()) {
                    Some(Ok(())) => {
                        debug!("Aborted transaction")
                    }
                    Some(Err(e)) => {
                        error!("Error when aborting a transaction (Bond drop): {e}");
                    }
                    None => {
                        error!("Critical error: RollbackAbort with no transaction open!?");
                    }
                }
            }
        }
    }

    fn start_transaction(&self) -> Result<Transaction<'a>, SqliteError> {
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

    fn handle_exec(&self, operation: OperationExec) {
        let connection = match self.transaction {
            Some(ref tx) => AgnosticConnection::Transaction(tx),
            None => AgnosticConnection::NotTransaction(self.connection),
        };

        match operation {
            OperationExec::Instruct(instruction) => {
                let res = instruction.run(&connection);
                if instruction.sender.send(res).is_err() {
                    // This means that the receiver has dropped.
                    error!("Oneshot error: Failed sending result back to caller");
                }
            }
            OperationExec::BatchedInsertReturningIds(instruct) => {
                let res = instruct.run(&connection);
                if instruct.sender.send(res).is_err() {
                    // This means that the receiver has dropped.
                    error!("Oneshot error: Failed sending result back to caller");
                }
            }
            OperationExec::Query(query) => {
                let res = query.run(&connection);
                if query.sender.send(res).is_err() {
                    // This means that the receiver has dropped.
                    error!("Oneshot error: Failed sending result back to caller");
                }
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
            return;
        }
        // Notify the main worker that the transaction has been rolled back
        self.queue
            .send(StashOperation::NotifyRollbackTransaction(self.id));
    }
}

/// Background worker for executing queries.
///
/// This struct provides a background worker for executing queries. It is
/// responsible for managing the connection pool and carrying out database
/// operations in a separate thread. It receives its instructions via a queue,
/// and sends the results back via oneshot channels.
///
/// There is no `new()` method for this struct, as it is created internally when
/// a worker thread is started. Hence the method to kick this off is called
/// [`start()`](Worker::start()), as it starts the worker on a thread, with a
/// new [`Worker`] instance, but returns associated data and not the [`Worker`]
/// instance itself.
///
/// Notably, everything the worker does is synchronous — it does not use async
/// at all.
///
#[derive(Debug)]
struct Worker {
    /// If a transaction is active, associated notifications will be held back
    /// in this buffer until the transaction is committed or rolled back, at
    /// which point they will be sent or discarded.
    notifications_buffer: HashMap<u64, Vec<Notification>>,

    /// The list of subscribers to the stash. This is used to send notifications
    /// whenever changes are made to the database.
    subscribers: Vec<(QueueSender<Notification>, Option<String>)>,
}

impl Worker {
    /// Starts a new background worker thread.
    ///
    /// This function creates a new [`Worker`] instance with a new SQLite
    /// connection pool, and starts the worker. This is run in a separate thread
    /// that is used to run blocking code, so it can execute queries in a
    /// non-blocking manner. The worker will execute queries sequentially, as
    /// they are received, and return the results via oneshot channels.
    ///
    /// The [`Worker`] instance is not returned by this function, and is kept
    /// internal to the functionality running on the background thread. This is
    /// because the [`PooledConnection`]s are not thread-safe.
    ///
    /// # Parameters
    ///
    /// * `path`     - The path to the SQLite database file. If `None`, an
    ///                in-memory database is created.
    /// * `receiver` - The receiving side of the worker's queue.
    /// * `stash`    - The [`Stash`] instance that the worker belongs to.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem creating
    /// the database or connection pool.
    ///
    #[allow(clippy::unnecessary_wraps)]
    fn start(receiver: QueueReceiver<StashOperation>) -> Result<(), StashError> {
        // Spawn a task to run the worker. This task will execute the queries
        // sequentially, as they are received, and will return the results via
        // oneshot channels.
        // There are no blocking operations here so you will not find any `spawn_blocking` call.
        _ = tokio::spawn(async move {
            let mut worker = Self {
                notifications_buffer: HashMap::new(),
                subscribers: Vec::new(),
            };

            while let Ok(operation) = receiver.recv_async().await {
                match operation {
                    StashOperation::NotifyCommitTransaction(id) => {
                        debug!(
                            "Stash: Publishing deferred Notification list for committed transaction ({id})",
                        );
                        if let Some(notifications) = worker.notifications_buffer.remove(&id) {
                            //TODO(ET-1400) - Proper unsubscribe support
                            debug!(
                                "Stash: Publishing {} notifications from Tether {id}",
                                notifications.len()
                            );
                            for notification in notifications {
                                #[allow(clippy::pattern_type_mismatch)]
                                for (subscriber, table) in &worker.subscribers {
                                    if table.as_ref().is_none_or(|t| t == &notification.table) {
                                        _ = subscriber.send(notification.clone());
                                    }
                                }
                            }
                            debug!("Notifications published from {id}");
                        } else {
                            // In theory this should never happen, but we also can't do anything with it
                            error!(
                                "Queue error: Failed to obtain Notification list for committed transaction"
                            );
                        }
                    }
                    StashOperation::Publish(notification) => {
                        if let Some(notifications) =
                            worker.notifications_buffer.get_mut(&notification.id)
                        {
                            debug!(
                                "Stash: Notification to publish (deferring, transaction {})",
                                notification.id
                            );
                            notifications.push(notification);
                        } else {
                            debug!("Stash: Notification to publish");
                            // Remove any subscribers that have perished.
                            // TODO(ET-1400): Proper unsubscribe API.
                            #[allow(clippy::pattern_type_mismatch)]
                            worker.subscribers.retain(|(s, _)| !s.is_disconnected());
                            for (subscriber, table) in &worker.subscribers {
                                if table.as_ref().is_none_or(|t| t == &notification.table) {
                                    // Because there is no way to unsubscribe right now
                                    // this can fail very frequently. We used to log the
                                    // errors here, but that can lead to log spam.
                                    _ = subscriber.send(notification.clone());
                                }
                            }
                        }
                    }
                    StashOperation::NotifyRollbackTransaction(trx_id) => {
                        debug!(
                            "Stash: Clearing deferred Notification list for aborted transaction"
                        );
                        drop(worker.notifications_buffer.remove(&trx_id));
                    }
                    StashOperation::NotifyStartTransaction(conn_handle) => {
                        debug!("Stash: Initializing deferred Notification list for transaction");
                        drop(worker.notifications_buffer.insert(conn_handle, vec![]));
                    }
                    StashOperation::Subscribe(subscription) => {
                        debug!("Stash: Subscription request");

                        let sub_queue = subscription.queue.clone();
                        let sub_table = subscription.table.clone();
                        worker.subscribers.push((sub_queue, sub_table));

                        // Although this operation is infallible, a response still needs to be sent,
                        // as the caller might be waiting on the oneshot channel in order to
                        // continue.
                        subscription.send(Ok(()));
                    }
                };
            }
        });

        Ok(())
    }
}

/// Prepares parameters ready to be used with a query.
///
/// This function prepares the parameters for a query, converting them into
/// a form that can be used with the [`rusqlite`] library.
///
/// # Parameters
///
/// * `params` - The parameters to prepare.
///
fn prepare_params(params: &[Box<dyn ToSql + Send>]) -> Vec<&dyn ToSql> {
    params
        .iter()
        .map(|p| {
            #[allow(clippy::shadow_same)]
            let p: &dyn ToSql = &**p;
            p
        })
        .collect()
}

/// Extension trait for the connection pool.
///
/// This trait provides extensions to the [`r2d2`] connection pool ([`Pool`]),
/// combining common behaviour and abstracting it away from the main library
/// code.
///
trait PoolExt<M: ManageConnection> {
    /// Gets a connection from the pool and subscribes to changes.
    ///
    /// This function gets a connection from the pool, and then subscribes to
    /// changes on the connection. Because the way [`rusqlite`] works is that
    /// its hooks only work in context to the same connection (i.e. any
    /// notifications of data changes made against a connection will only be
    /// sent to the registered callback hook for that connection), we need to
    /// ensure that all connections are subscribed to changes.
    ///
    /// By centralising this logic and calling it in preference to the standard
    /// [`get()`](Pool::get()) method, we ensure that all connections are set up
    /// to receive notifications of changes.
    ///
    /// The notifications are sent to the central worker via its standard
    /// operations queue, whereupon it will then redistribute them to any
    /// registered subscribers.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The queue to send the [`Notification`]s to. This is
    ///                   the standard [`Operation`]s queue of the central
    ///                   worker.
    /// * `conn_handle` - The handle of the associated connection. This is used
    ///                   here to provide context to the notifications. It is
    ///                   passed in as a weak reference so that the closure does
    ///                   not prevent clean-up. Ad-hoc queries will not have an
    ///                   associated connection handle.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem getting
    /// a connection from the pool.
    ///
    fn get_and_subscribe(
        &self,
        queue: QueueSender<StashOperation>,
        id: u64,
    ) -> Result<PooledConnection<M>, StashError>;
}

impl PoolExt<SqliteConnectionManager> for Pool<SqliteConnectionManager> {
    fn get_and_subscribe(
        &self,
        queue: QueueSender<StashOperation>,
        id: u64,
    ) -> Result<PooledConnection<SqliteConnectionManager>, StashError> {
        let t1 = Instant::now();
        let connection = self.get().map_err(StashError::TetherError)?;
        connection.update_hook(Some(
            move |action: Action, _db_name: &str, table_name: &str, row_id: i64| {
                #[allow(clippy::cast_sign_loss)]
                if queue
                    .send(StashOperation::Publish(Notification {
                        action,
                        table: table_name.to_owned(),
                        row: row_id as u64,
                        id
                    }))
                    .is_err()
                {
                    error!("Queue error: Failed to publish a Notification to the worker thread. Elapsed: {:?}", t1.elapsed());
                }
            },
        ));
        Ok(connection)
    }
}

/// Converts the query results into the desired type.
///
/// This is necessary because the [`Rows`] type returned by the [`rusqlite`]
/// library is not thread-safe. We only need one converter function, but the key
/// is that the context of the generic type `T` is established at the point this
/// function is used by [`Stash::query()`] and [`Tether::query()`] and passed
/// through the queue to [`Query::run()`].
///
/// Notably, we cannot really get away from use of `Box<dyn Any>` here, as we
/// need to be able to return a collection of any type that implements the
/// [`DbRecord`] trait. We don't want to restrict the caller to a specific type,
/// or even an enumerated list of types, and neither to we want to serialise the
/// results into intermediary form to unpack at the other end of the queue. We
/// therefore use `Box<dyn Any>` for a very short and specific purpose, which is
/// to send the results back to the caller via the oneshot channel. They have in
/// fact already been converted at this point, but must be passed generically
/// and then downcast. This method of transport is therefore the most efficient
/// option we can choose, and bears a very slight overhead of type manipulation,
/// but does not introduce any wider dynamic dispatch or unnecessary byte
/// manipulation (as the deserialisation happens exactly once).
///
/// # Parameters
///
/// * `rows`  - The rows of data returned by the query. These will be converted
///             to the type specified by `T`.
/// * `stash` - The associated [`Stash`] instance from which the rows were
///             obtained.
///
/// # Errors
///
/// A [`ConversionError`] is returned if there is a problem deserialising the
/// query results or performing any type conversions as part of the overall
/// row-deserialisation process. This will then be converted into a
/// [`StashError::DeserializationError`] by the caller.
///
#[allow(clippy::needless_pass_by_value)]
fn converter<T>(rows: Rows<'_>) -> Result<DbRecords, ConversionError>
where
    T: DbRecord + Send + 'static,
    DbRecords: FromIterator<Box<T>>,
{
    Ok(from_rows::<T>(rows)?.into_iter().map(Box::new).collect())
}

/// Value record struct used to generate the `DbRecord` glue code.
#[derive(Debug, DbRecord, Clone, PartialEq)]
struct ValueRecord<V: Clone + Debug + FromSql + ToSql + Send + Sync + PartialEq + 'static> {
    /// Value we wish to read from the query.
    #[DbField]
    value: V,
}
