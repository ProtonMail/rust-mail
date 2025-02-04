#![allow(clippy::struct_field_names)]

//! Main database-handling interface.
//!
//! This module provides the main functionality for handling the database. At
//! present there is only support for SQLite, and no attempt is made to cater
//! for any other database engine. Hence the behaviour of this module is
//! closely tied to SQLite.
//!
//! # Choice of name
//!
//! The name "stash" has been chosen carefully — it can represent a place where
//! data is stored, and it does not already appear in the repository. This makes
//! refactoring clearer, establishing the new identity, after which the name can
//! be trivially changed to e.g. "database".
//!
//! Equally, the term "tether" has been used for the same reason. It represents
//! the connection established to the database, and it is not already in use in
//! the repository.
//!
//! # Design
//!
//! The primary point of interaction is the [`Stash`] struct, which provides a
//! centralised, asynchronous database-handling interface that manages
//! connections and carries out queries. One [`Stash`] instance should be
//! created per database, and can be cloned and shared across threads as
//! necessary.
//!
//! When interacting with the database, there are two choices:
//!
//!   1. Use the functionality on the [`Stash`] struct directly, i.e. by calling
//!      the [`Stash::query()`] or [`Stash::execute()`] methods. This will run
//!      each query against a new connection, saving that step for the caller.
//!
//!   2. Obtain a connection from the pool, and then use the equivalent
//!      functionality it provides, i.e. by calling the [`Tether::query()`] or
//!      [`Tether::execute()`] methods. This will allow for multiple queries to
//!      be run on the same connection, which is necessary for transactions.
//!
//! The first approach simply combines the steps of obtaining a new connection
//! and then executing a query, with every query being run on a new connection.
//! When using transactions, all related queries should be run on the same
//! connection, in which case it is necessary to separate the steps.
//!
//! Connections are provided via lightweight, thread-safe [`Tether`]s, which are
//! use in place of the "real" connections, as those are not thread-safe. The
//! [`Tether`] struct offers the same query interface as a [`Stash`] instance,
//! but is tied to a specific connection. It is tied through the issuing of a
//! unique internal handle, which is immutable, and automatically expires when
//! no longer used.
//!
//! Under the bonnet, there is a background worker that manages the connection
//! pool and executes queries. This worker runs on a separate thread, and
//! receives its instructions via a queue. This ensures that all operations are
//! executed sequentially, and that connections are managed and made
//! thread-safe.
//!
//! # Approach to async
//!
//! The [`Stash`] struct is designed to be used in an asynchronous context. The
//! [`query()`][Stash::query()] and [`execute()`][Stash::execute()] methods are
//! asynchronous (as are their connection-specific [`Tether`] counterparts), and
//! the [`Stash`] struct itself is cloneable and shareable across threads. The
//! database handling uses the [`r2d2`] and [`rusqlite`] crates, which are
//! synchronous, so they are handled in a separate background thread by a worker
//! to avoid blocking the main Tokio runtime, and to ensure that there is a
//! synchronous "funnel" to handle all database operations.
//!
//! As the various [`rusqlite`] types are not [`Send`] compatible, they cannot
//! be passed between threads, and so cannot cross the async boundary. Therefore
//! this approach of the background worker and the [`Tether`] struct is
//! necessary to provide a thread-safe and async-compatible interface to the
//! database.
//!
//! The main worker processes the incoming queries and other database operations
//! via an MPSC queue. As soon as it picks a query up from the queue it hands it
//! over to another worker on a separate thread for processing. If the query is
//! a once-off, i.e. does not need to re-use a database connection, then it is
//! executed in an async thread, and the [`spawn_blocking()`] function is used
//! to run the blocking synchronous code in a separate thread. This allows the
//! Tokio runtime to continue running other tasks while the blocking code is
//! running.
//!
//! This is important because otherwise the executor would be blocked, and Tokio
//! would not be able to run other tasks. To clarify: the mechanism by which the
//! Tokio runtime operates is that of work scheduling. It will run the various
//! work units (tasks) that it has against the available OS threads, via
//! allocated "core" threads, and will switch between them (i.e. between the
//! tasks) as necessary, allocating the tasks against the core threads according
//! to its work management priorities. If a task blocks, then the thread that it
//! is running on will be blocked, and the Tokio runtime will not be able to run
//! other tasks on that thread.
//!
//! Bear in mind that asking Tokio to create a new "thread" is not the same as
//! creating a new OS thread. Tokio uses a thread pool, and manages the work
//! units, each of which *can* operate on a separate thread, allocating the work
//! units to the available core OS threads as needed. For this reason, it is
//! important to notify the Tokio runtime when a task is going to issue a
//! blocking call (e.g. waiting on file or network I/O), or perform a lot of
//! compute without yielding. Such a situation can prevent the executor from
//! driving other tasks forward, and can lead to a deadlock. Notifying the
//! executor allows it to hand off any other tasks it has to a new core thread
//! before the blocking call is made. Tokio handles blocking situations
//! separately, in blocking threads, which are separate from the core threads.
//!
//! Tokio has two kinds of threads in its thread pool: core (OS) threads, and
//! blocking threads. By default, Tokio will create one core thread for each CPU
//! core, and up to around 500 blocking threads. Using [`block_in_place()`](tokio::task::block_in_place())
//! temporarily *changes* the current thread category from core to blocking,
//! allowing the runtime to spawn another core thread to handle things while the
//! blocking code runs. Because the whole thread categorisation is changed,
//! anything else (i.e. other tasks) associated with the thread are taken with
//! it. Whereas, [`spawn_blocking()`] sends the *task* to a thread in the
//! blocking category, allowing the other associated tasks to continue.
//!
//! The two main ways of notifying the Tokio runtime that a task is blocking are
//! [`block_in_place()`](tokio::task::block_in_place()) and
//! [`spawn_blocking()`]. The difference is that [`block_in_place()`](tokio::task::block_in_place())
//! blocks the current core thread, whereas [`spawn_blocking()`] spawns a new
//! thread *request* to run the blocking code. Both allow the Tokio runtime to
//! continue running other tasks, and allow the executor to continue in general,
//! but [`block_in_place()`](tokio::task::block_in_place()) will hold up any
//! other tasks running on the current thread, and will prevent the thread from
//! being used for anything else until the work completes.
//!
//! It is always importance to consider performance, efficiency, and resource
//! availability when designing asynchronous code. Improper use can lead to
//! exhaustion, starvation, and deadlocks. We do not have to worry about thread
//! pool exhaustion, because Tokio will spawn more blocking threads until the
//! upper limit is reached, after which, the tasks are put into a queue. That
//! means we are free to request new threads as new database queries arise,
//! without concern.
//!
//! As a rule of thumb, async code should never run for too long between `await`
//! occurrences. This is because the Tokio runtime uses cooperative scheduling,
//! and will not interrupt a task that is running. Hence care should be taken to
//! identify those places that may block, especially when using synchronous
//! libraries. On the other hand, over-use of async can cause performance
//! degradation due to the overhead of task management, mainly the time taken to
//! switch tasks between threads. Notably, it is in this area that Go tends to
//! outperform Rust, because Go uses a different threading model with
//! goroutines. The Tokio approach of essentially hibernating and reviving tasks
//! is more complex, but allows for more fine-grained control and better
//! resource management, and increased predictability and confidence. Therefore,
//! it is important to only make async those functions that need to be async
//! (bearing in mind the "polluting" effect of async on the codebase), and not
//! to just make everything async by default. In reality, providing these basic
//! guidelines are followed, operational issues are rare, and performance is
//! generally very good.
//!
//! Note that due to the async-safe implementation, there is no need to use the
//! [`spawn_blocking()`] function in calling code. It is use where necessary
//! internally. These notes are provided for general information and context,
//! and to guide future development.
//!
//! # Thread structure and management
//!
//! It is worth describing the thread structure and management in more detail.
//! The module as a whole is thread-safe and compatible with both async usage
//! and multi-threading in general. What this means is that it is possible to
//! interact with the same [`Stash`] instance from multiple threads.
//!
//! Calling code runs on the main Tokio runtime, and issues async requests to
//! the main interface functions of the [`Stash`] struct (such as
//! [`Stash::query()`] and [`Stash::execute()`]). These functions will then
//! send instructions to the background worker via an MPSC queue, and will
//! obtain their responses via oneshot channels, and pass them back to the
//! caller. In this way, all of this behaviour is invisible to the caller, and
//! the interface is simple and easy to use.
//!
//! The background worker runs as sync on a dedicated thread, and processes the
//! incoming instructions from the queue as they arrive. These are the main
//! points of operation:
//!
//!   - Database operation instructions get sent via the central queue. The
//!     sending is done as async by the sender, with the sender here being the
//!     public interface methods used by the caller.
//!
//!   - A central worker listens to the queue and takes the instructions from
//!     it. This is sync. It could also be async, but there is no specific need
//!     for this at present.
//!
//!   - The central worker then looks at the instruction it has received:
//!
//!       - If it is not associated to any connection then it spawns an async
//!         thread block to handle it. This will be managed by Tokio. Within the
//!         spawned async thread the call to the (sync) database operation is
//!         run with [`spawn_blocking()`].
//!
//!       - If it is associated to a connection, and the connection is new, then
//!         it creates a new dedicated sync thread (non-Tokio) to handle it.
//!         This is registered with a thread handle for future use, and a
//!         channel is established for communication.
//!
//!       - If it is associated to a connection, and the connection already
//!         exists, then it sends the instruction down the channel to that
//!         thread so that it can carry out its instructions. This way, the
//!         [`PooledConnection`] established inside the thread is preserved and
//!         re-used.
//!
//!     In this way, the central worker is never blocked, and can continue to
//!     process instructions as they arrive.
//!
//!   - Each persistent, connection-specific thread is considered to be active
//!     when a new instruction is sent to it, and once that action has been
//!     completed, it should inform the central worker that it has finished.
//!     This will update a last-active time.
//!
//!   - Garbage collection will run at intervals by the central worker. This
//!     looks for expired connection handlers (tethers), and removes the
//!     associated thread if it is inactive. Additionally, it looks for threads
//!     that have been inactive for some time, and prunes them, logging a
//!     warning.
//!
//!   - The maximum number of connection-based threads to spawn is configurable.
//!     If this limit is hit then more connections will not be created, but
//!     instead the instructions will be added to a Deque held by the central
//!     worker, up to a certain limit. Beyond that limit, additional
//!     instructions will be rejected with errors. Otherwise, the worker will
//!     resume processing the Deque once spare threads are available again.
//!
//!   - The number of active transactions is monitored, and should be less than
//!     the allowed connection thread limit, otherwise errors will be thrown.
//!
//!   - Nested transactions will be detected, and rejected. This is achieved by
//!     each queued instruction having a thread identifier. If thread X starts a
//!     transaction, and then later there is another request from thread X to
//!     start another transaction before the first one has finished (regardless
//!     of the connection context), then this will be rejected. This mechanism
//!     is fully-async safe, and the method of identifying threads "follows" the
//!     logic trail through async/await boundaries.
//!
//! # Performance
//!
//! The current implementation has been carefully designed to be suitable for
//! the target usage. The profile is fairly write-heavy, and quite low volume.
//! Although performance is important to consider, the public interface and
//! manner of approach are of paramount importance in order to provide ease of
//! development and reliability of operation. Top priorities are therefore
//! async compatibility, simplicity of use, predictability, and robustness of
//! operation. The performance of the system is not expected to be a limiting
//! factor, and the current design is expected to be more than adequate for the
//! target usage.
//!
//! With that said, we can make educated predictions about scalability and where
//! constraints may occur. The current design is expected to be able to handle
//! significant volume without issue, but the approach of funnelling all queries
//! through a single worker thread is a potential bottleneck. This can be
//! improved or resolved by adding additional workers to process the queue, but
//! that may or may not be desirable. The fact that the main queue-processing
//! worker is very lightweight and non-blocking, and simply hands off the actual
//! query execution to separate threads, means that it is unlikely to become a
//! source of contention.
//!
//! The approach to logic using this module also needs to be thought through
//! carefully in any situation where transactions are used. As a rule of thumb,
//! code using transactions should be as close to hand as possible (to minimise
//! unseen effects), and should keep the transaction open for as short a time as
//! possible.
//!
//! The following points of operation need to generally be considered:
//!
//!   1. **Is it possible for a deadlock to occur because multiple threads are
//!      waiting for interdependent queries to complete?**
//!
//!      This should not generally be a concern, as most queries will be run on
//!      new connections, and only transactions that have started write
//!      operations need to actually be thought about (see question 2 below).
//!      For the vast majority of usage, developers using this module will
//!      therefore not need to think at all about deadlocks or the order of
//!      operations. The approach taken is as non-blocking as possible.
//!
//!      The only time when this could be a concern is when using transactions,
//!      in which case the developer will know that all queries within the
//!      transaction need to be run on the same connection, and so will use the
//!      [`Tether`] interface to ensure that this is the case. Transactions have
//!      a blocking effect, but notably, any ad-hoc, unrelated queries will not
//!      be run on the same connection, and so will not be blocked unless a
//!      write operation has started (see question 2 below). Deadlocks can
//!      potentially occur in this situation, but that will not be due to
//!      multiple threads waiting for interdependent queries to complete, but
//!      rather, due to logic happening in the same thread.
//!
//!      Note that any queries happening *inside* an active transaction — i.e.
//!      read or write, and reusing the same connection — will not be blocked,
//!      as the transaction holds the lock. So **a deadlock situation is limited
//!      to a situation where a query is attempted by the same thread that has
//!      started a write transaction, but on a different connection.**
//!
//!   2. **Can an active transaction block other queries?**
//!
//!      Yes. An active transaction will apply to its own connection, and will
//!      hold up any effects of changes carried out on that connection until it
//!      is committed. Other, unrelated queries will not initially be affected,
//!      as they will be run on new connections. However, as soon as a write
//!      operation occurs within the transaction, all unrelated queries will be
//!      blocked until the transaction is committed or rolled back.
//!
//!        - In SQLite, when a transaction is started, it does not immediately
//!          acquire any locks.
//!
//!        - When a read operation is carried out it then establishes a shared
//!          read lock on the database. Multiple read transactions can coexist
//!          and proceed concurrently, as they can share the read lock.
//!
//!        - However, when a transaction performs a write operation (`INSERT`,
//!          `UPDATE`, `DELETE`), it attempts to acquire a reserved write lock
//!          on the database. From that point on, **any other queries that are
//!          attempted while a write transaction is active will block until the
//!          write transaction has completed** and released the reserved write
//!          lock.
//!
//!        - Similarly, **if a new transaction is attempted while there is
//!          another, active write transaction, it will block until the write
//!          transaction has completed** and released the reserved write lock.
//!
//!      This module could take the approach of disallowing multiple
//!      simultaneous transactions, but given the inherently asynchronous nature
//!      of the system (with a number of simultaneous sources of activity, all
//!      of which could potentially lead to a transaction), this would lead to a
//!      non-trivial number of rejections and errors, which would not be a good
//!      user experience. Instead, the approach is taken whereby "nested"
//!      transactions are disallowed (see question 3 below).
//!
//!      In summary:
//!
//!        - SQLite transactions operate at the database file level, not at a
//!          connection, table, or resource level.
//!
//!        - Read transactions don't block reads, but do block writes.
//!
//!        - Write transactions will block reads and writes.
//!
//!      Reference:
//!
//!        - [File Locking And Concurrency In SQLite Version 3](https://www.sqlite.org/lockingv3.html)
//!
//!          *The SQL command "BEGIN TRANSACTION" ... is used to take SQLite out
//!          of autocommit mode. Note that the BEGIN command does not acquire
//!          any locks on the database. After a BEGIN command, a SHARED lock
//!          will be acquired when the first SELECT statement is executed. A
//!          RESERVED lock will be acquired when the first INSERT, UPDATE, or
//!          DELETE statement is executed. No EXCLUSIVE lock is acquired until
//!          either the memory cache fills up and must be spilled to disk or
//!          until the transaction commits. In this way, the system delays
//!          blocking read access to the file until the last possible moment.*
//!
//!   3. **Is it possible to carry out nested transactions?**
//!
//!      At present, these are disallowed.
//!
//!      For the sake of clarity, a "nested" transaction is considered to be one
//!      where a new transaction is started from the same thread as an existing,
//!      active transaction. This is not allowed in this module, as it is not
//!      currently considered valid behaviour to be in the process of carrying
//!      out changes that require a transaction, and then to do something that
//!      itself also requires a transaction. This position may change in future,
//!      as it is not technically invalid, only considered to be logically so.
//!      If and when that position changes, this module will need to be changed
//!      to not only allow nested transactions, but to handle them correctly in
//!      order to prevent deadlocks.
//!
//!      If other threads attempt transactions then that is absolutely fine, and
//!      although they will be blocked until the current active transaction
//!      completes, they will not be rejected. Additionally, as they are for
//!      unrelated logic, their being blocked will not lead to any deadlocks.
//!
//!      However, the real issue is that of nested unrelated queries, i.e. when
//!      a transaction has started and then another piece of code on the same
//!      thread attempts a query (not just a transaction — any query) on a
//!      different connection. That can lead to a deadlock, and so care needs to
//!      be taken (see question 2 above).
//!
//!   4. **Does the synchronous, single-threaded nature of the background worker
//!      cause any reduction in performance, and does it prevent parallel read
//!      operations?**
//!
//!      The current design is expected to be more than adequate for the target
//!      usage. The central background worker that handles the queue hands off
//!      the actual query execution to separate threads, and so does not itself
//!      block. Read operations can therefore occur in parallel, as the actual
//!      query handling is multi-threaded.
//!

use crate::orm::{from_rows, perform_load, ConversionError, DbRecord, DbRecords, Model};
use core::fmt;
use core::fmt::Debug;
use core::future::Future;
use core::mem;
use core::ops::Deref;
use core::ptr::null;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;
use core::time::Duration;
use flume::{Receiver as QueueReceiver, Sender as QueueSender};
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
use std::collections::{hash_map::Entry, HashMap};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Weak};
use std::thread::{spawn, JoinHandle};
use std::time::Instant;
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tokio::task::spawn_blocking;
use tracing::{debug, error, warn};
// Used to resolve undeclared crate of module `stash` from DbRecord proc marco
use crate as stash;
use crate::registry::{StashRegistry, REGISTRY};

/// Set a timeout for a specified amount of time when a table is locked. This
/// defaults to 5,000 milliseconds in the underlying libraries, but can be
/// overridden here if necessary.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The maximum number of simultaneous connections allowed to the database. This
/// defaults to 10.
const MAX_CONNECTIONS: u32 = 100;

/// A type alias for a field convertor function.
type Convertor = Box<dyn Fn(Rows<'_>) -> Result<DbRecords, ConversionError> + Send>;

/// A dual-nature connection wrapper.
///
/// This enum allows transparent handling of a connection, whether or not a
/// transaction is currently active. It is used only for representation of types
/// owned elsewhere, hence wraps references and borrows those instances.
///
/// It implements [`Deref`] so that it is essentially invisible to the caller.
///
enum AgnosticConnection<'tx> {
    /// A connection that is not currently in a transaction.
    Unbound(&'tx PooledConnection<SqliteConnectionManager>),

    /// A connection that is currently engaged in an active transaction.
    Engaged(&'tx Transaction<'tx>),
}

impl Deref for AgnosticConnection<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        match *self {
            Self::Unbound(connection) => connection,
            Self::Engaged(transaction) => transaction,
        }
    }
}

/// The types of database operation that can be performed by the background
/// worker.
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
/// * [`Information`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`OperationLogic`]
/// * [`Query`]
/// * [`Subscription`]
/// * [`Worker`]
///
enum Operation {
    /// Terminate everything.
    Quit,
    /// Closes a connection. This will also cause the associated
    /// [`TetheredWorker`] to exit.
    CloseConnection(Command),

    /// Commits a transaction, i.e. finalises it.
    CommitTransaction(Command),

    /// A query to be executed, where no results are expected. This is usually
    /// a write query, or a command, but differentiation is up to the caller and
    /// not enforced.
    Instruct(Instruction),

    /// Notify a transaction was commited.
    NotifyCommitTransaction(Arc<AtomicU32>),

    /// Notify a transaction was rolled back.
    NotifyRollbackTransaction(Arc<AtomicU32>),

    /// Notify a new transaction has started.
    NotifyStartTransaction(Arc<AtomicU32>),

    /// Publishes a notification of changes made to the database to all
    /// subscribers.
    Publish(Notification),

    /// A query to be executed, where results are expected. This is typically a
    /// read query, but could be any query where results are expected, such as
    /// an `INSERT` query that returns the ID of the inserted row.
    Query(Query),

    /// Rolls back a transaction, i.e. abandons it.
    RollbackTransaction(Command),

    /// Starts a new transaction.
    StartTransaction(Command),

    /// Subscribes to notifications of changes made to the database.
    Subscribe(Subscription),
}

impl Operation {
    /// Sends an error result back to the caller.
    ///
    /// This is a convenience function to reduce code boilerplate, sending an
    /// error result back to the caller via the oneshot channel.
    ///
    /// # Parameters
    ///
    /// * `error` - The error to send back to the caller.
    /// * `stash`  - The associated [`Stash`] instance for the operation.
    ///
    fn send_back_error(&mut self, error: StashError) {
        match *self {
            Self::CloseConnection(ref mut command)
            | Self::CommitTransaction(ref mut command)
            | Self::RollbackTransaction(ref mut command)
            | Self::StartTransaction(ref mut command) => command.send_back(Err(error)),
            Self::Instruct(ref mut instruction) => instruction.send_back(Err(error)),
            Self::Publish(_)
            | Self::NotifyRollbackTransaction(_)
            | Self::NotifyCommitTransaction(_)
            | Self::Quit
            | Self::NotifyStartTransaction(_) => {}
            Self::Query(ref mut query) => query.send_back(Err(error)),
            Self::Subscribe(ref mut subscription) => subscription.send_back(Err(error)),
        }
    }
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
    #[error("Transaction error: {0}")]
    TransactionError(SqliteError),

    /// Custom error that can be returned when an error occurs during
    /// implementations of `on_save()` or `on_load()` for [`Model`].
    #[error("Custom: {0}")]
    Custom(String),
}

/// A command operation to be executed by the worker.
///
/// This is used for system-defined operations (i.e. those where the user does
/// not define the query) such starting a new transaction.
///
/// # See also
///
/// * [`Information`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`Operation`]
/// * [`Query`]
/// * [`Subscription`]
///
struct Command {
    /// The unique global identifier of the command, relative to the [`Stash`]
    /// instance it is associated with.
    id: u64,

    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: Option<OneshotSender<Result<(), StashError>>>,

    /// The unique handle of the connection to use for the query. If [`Some`] a
    /// database connection will be created and associated if not already
    /// registered, and re-used otherwise. If [`None`], a new database
    /// connection will be created, but not registered, and used just this once.
    conn_handle: Option<Arc<AtomicU32>>,

    /// The time at which the operation started.
    start_time: Instant,
}

impl Command {
    /// Creates a new command operation.
    ///
    /// # Parameters
    ///
    /// * `stash`       - The associated [`Stash`] instance for the operation.
    /// * `channel`     - The communication channel used to send the result of
    ///                   the operation back to the caller.
    /// * `conn_handle` - The unique handle of the connection to use for the
    ///                   query. If [`Some`] a database connection will be
    ///                   created and associated if not already registered, and
    ///                   re-used otherwise. If [`None`], a new database
    ///                   connection will be created, but not registered, and
    ///                   used just this once.
    ///
    fn new(
        channel: Option<OneshotSender<Result<(), StashError>>>,
        conn_handle: Option<Arc<AtomicU32>>,
    ) -> Self {
        let id = TOTAL_COMMANDS_RUN.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            channel,
            conn_handle,
            start_time: Instant::now(),
        }
    }
}

/// This is used to assign ids to [`Command`]s
pub static TOTAL_COMMANDS_RUN: AtomicU64 = AtomicU64::new(0);

impl OperationLogic for Command {
    type Output = ();

    fn channel(&mut self) -> Option<OneshotSender<Result<Self::Output, StashError>>> {
        self.channel.take()
    }

    /// Carries out a command.
    ///
    /// **Note: This function does not actually do anything, as the operational
    /// context for commands is the [`Operation`] variant they are wrapped in.**
    ///
    /// # Parameters
    ///
    /// * `connection` - The database connection to use for the operation.
    /// * `stash`      - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// None.
    ///
    fn run(&self, _connection: &AgnosticConnection<'_>) -> Result<(), StashError> {
        Ok(())
    }

    fn start_time(&self) -> Instant {
        self.start_time
    }
}

/// An operation to be executed by the worker, which does not return any data.
///
/// This is used for operations such as `INSERT`, `UPDATE`, and `DELETE`, where
/// the result is the number of rows affected, along with other similar
/// commands.
///
/// # See also
///
/// * [`Command`]
/// * [`Information`]
/// * [`Notification`]
/// * [`Operation`]
/// * [`Query`]
/// * [`Subscription`]
///
struct Instruction {
    /// The unique global identifier of the instruction, relative to the
    /// [`Stash`] instance it is associated with.
    id: u64,

    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: Option<OneshotSender<Result<usize, StashError>>>,

    /// The unique handle of the connection to use for the query. If [`Some`] a
    /// database connection will be created and associated if not already
    /// registered, and re-used otherwise. If [`None`], a new database
    /// connection will be created, but not registered, and used just this once.
    conn_handle: Option<Arc<AtomicU32>>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,

    /// The time at which the operation started.
    start_time: Instant,
}

impl Instruction {
    /// Creates a new command operation.
    ///
    /// # Parameters
    ///
    /// * `stash`       - The associated [`Stash`] instance for the operation.
    /// * `channel`     - The communication channel used to send the result of
    ///                   the operation back to the caller.
    /// * `conn_handle` - The unique handle of the connection to use for the
    ///                   query. If [`Some`] a database connection will be
    ///                   created and associated if not already registered, and
    ///                   re-used otherwise. If [`None`], a new database
    ///                   connection will be created, but not registered, and
    ///                   used just this once.
    /// * `query`       - The query to execute. This is in raw SQL format ready
    ///                   for parameter substitution.
    /// * `params`      - The parameters to pass to the query. These are boxed
    ///                   trait objects that implement the [`ToSql`] trait, and
    ///                   are `Send` so that they can be sent between threads.
    fn new(
        channel: Option<OneshotSender<Result<usize, StashError>>>,
        conn_handle: Option<Arc<AtomicU32>>,
        query: String,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Self {
        let id = TOTAL_COMMANDS_RUN.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            channel,
            conn_handle,
            params,
            query,
            start_time: Instant::now(),
        }
    }
}

impl OperationLogic for Instruction {
    type Output = usize;

    fn channel(&mut self) -> Option<OneshotSender<Result<Self::Output, StashError>>> {
        self.channel.take()
    }

    /// Prepares and executes a query, and returns the number of affected rows.
    ///
    /// This function prepares a query and executes it on the database, and then
    /// indicates whether it was successful, returning the number of affected
    /// rows.
    ///
    /// **Note: This function is the one that actually deals with the query
    /// execution, which occurs on the background worker thread in response to
    /// queued instructions. It is an internal function. For the public-facing
    /// versions of this function, which lead to it being called, see
    /// [`Stash::execute()`] and [`Tether::execute()`].**
    ///
    /// # Parameters
    ///
    /// * `connection` - The database connection to use for the operation.
    /// * `stash`      - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`ExecutionError`](StashError::ExecutionError) - Problem executing
    ///     the query.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///
    /// # See also
    ///
    /// * [`Query::run()`]
    /// * [`Stash::execute()`]
    /// * [`Tether::execute()`]
    ///
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<usize, StashError> {
        let mut statement = connection
            .prepare(&self.query)
            .map_err(StashError::PreparationError)?;
        let affected = statement
            .execute(&*Self::prepare_params(&self.params))
            .map_err(StashError::ExecutionError)?;
        if let Some(query) = statement.expanded_sql() {
            debug!("Query: {query}");
        }
        Ok(affected)
    }

    fn start_time(&self) -> Instant {
        self.start_time
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
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Notification {
    /// The handle of the associated connection. This is a strong reference in
    /// order to ensure the connection is not dropped while the notification is
    /// being processed. If the query was ad-hoc, then this will be [`None`].
    pub conn_handle: Option<Arc<AtomicU32>>,

    /// The action that has been performed on the table. This can be one of
    /// `INSERT`, `UPDATE`, or `DELETE`.
    pub action: Action,

    /// The name of the table that the action was performed on, i.e. that has
    /// changed.
    pub table: String,

    /// The row ID of the row that has been acted on, i.e. changed. This may or
    /// may not be useful.
    pub row: u64,
}

impl Eq for Notification {}

impl PartialEq for Notification {
    fn eq(&self, other: &Self) -> bool {
        #[allow(clippy::pattern_type_mismatch)]
        let conn_handle_equal = match (&self.conn_handle, &other.conn_handle) {
            (Some(self_handle), Some(other_handle)) => Arc::ptr_eq(self_handle, other_handle),
            (None, None) => true,
            _ => false,
        };
        conn_handle_equal
            && self.action == other.action
            && self.table == other.table
            && self.row == other.row
    }
}

/// An operation to be executed by the worker, which returns data.
///
/// This is used for operations such as `SELECT`, where the result is a set of
/// rows of data. Notably, the deserialisation function is also passed, so that
/// the results can be converted into the desired type. This is because the
/// [`Rows`] type returned by the [`rusqlite`] library is not thread-safe.
///
/// # See also
///
/// * [`Command`]
/// * [`Information`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`Operation`]
/// * [`Subscription`]
///
struct Query {
    /// The unique global identifier of the query, relative to the [`Stash`]
    /// instance it is associated with.
    id: u64,

    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: Option<OneshotSender<Result<DbRecords, StashError>>>,

    /// The unique handle of the connection to use for the query. If [`Some`] a
    /// database connection will be created and associated if not already
    /// registered, and re-used otherwise. If [`None`], a new database
    /// connection will be created, but not registered, and used just this once.
    conn_handle: Option<Arc<AtomicU32>>,

    /// The deserialisation function to use to convert the query results into
    /// the desired type. This is necessary because the [`Rows`] type returned
    /// by the [`rusqlite`] library is not thread-safe.
    converter: Convertor,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,

    /// The time at which the operation started.
    start_time: Instant,
}

impl Query {
    /// Creates a new command operation.
    ///
    /// # Parameters
    ///
    /// * `stash`       - The associated [`Stash`] instance for the operation.
    /// * `channel`     - The communication channel used to send the result of
    ///                   the operation back to the caller.
    /// * `conn_handle` - The unique handle of the connection to use for the
    ///                   query. If [`Some`] a database connection will be
    ///                   created and associated if not already registered, and
    ///                   re-used otherwise. If [`None`], a new database
    ///                   connection will be created, but not registered, and
    ///                   used just this once.
    /// * `query`       - The query to execute. This is in raw SQL format ready
    ///                   for parameter substitution.
    /// * `params`      - The parameters to pass to the query. These are boxed
    ///                   trait objects that implement the [`ToSql`] trait, and
    ///                   are `Send` so that they can be sent between threads.
    /// * `converter`   - The deserialisation function to use to convert the
    ///                   query results into the desired type. This is necessary
    ///                   because the [`Rows`] type returned by the [`rusqlite`]
    ///                   library is not thread-safe.
    ///
    fn new(
        channel: Option<OneshotSender<Result<DbRecords, StashError>>>,
        conn_handle: Option<Arc<AtomicU32>>,
        query: String,
        params: Vec<Box<dyn ToSql + Send>>,
        converter: Convertor,
    ) -> Self {
        let id = TOTAL_COMMANDS_RUN.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            channel,
            conn_handle,
            converter,
            params,
            query,
            start_time: Instant::now(),
        }
    }
}

impl OperationLogic for Query {
    type Output = DbRecords;

    fn channel(&mut self) -> Option<OneshotSender<Result<Self::Output, StashError>>> {
        self.channel.take()
    }

    /// Prepares and executes a query, and returns any rows of data emitted.
    ///
    /// This function prepares a query and executes it on the database, and then
    /// indicates whether it was successful, returning the number of affected
    /// rows.
    ///
    /// **Note: This function is the one that actually deals with the query
    /// execution, which occurs on the background worker thread in response to
    /// queued instructions. It is an internal function. For the public-facing
    /// versions of this function, which lead to it being called, see
    /// [`Stash::query()`] and [`Tether::query()`].**
    ///
    /// # Parameters
    ///
    /// * `connection` - The database connection to use for the operation.
    /// * `stash`      - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// The following [`StashError`] variants can be returned:
    ///
    ///   - [`DeserializationError`](StashError::DeserializationError) - Problem
    ///     converting from [`Rows`] to `T`.
    ///   - [`ExecutionError`](StashError::ExecutionError) - Problem executing
    ///     the query.
    ///   - [`PreparationError`](StashError::PreparationError) - Problem
    ///     preparing the query.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///
    /// # See also
    ///
    /// * [`Instruction::run()`]
    /// * [`Stash::query()`]
    /// * [`Tether::query()`]
    ///
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<DbRecords, StashError> {
        let mut statement = connection
            .prepare(&self.query)
            .map_err(StashError::PreparationError)?;
        let rows: Result<DbRecords, ConversionError> = (self.converter)(
            statement
                .query(&*Self::prepare_params(&self.params))
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

    fn start_time(&self) -> Instant {
        self.start_time
    }
}

/// Database interaction interface.
///
/// This struct provides a centralised database-handling interface that
/// manages connections and carries out queries.
///
/// [`Stash`] instances are lightweight, and can be freely cloned without any
/// concerns. When cloned, the new instance will share the same queue — and
/// therefore worker — as the original. This is achieved due to the nature of
/// the [`QueueSender`] type, which is thread-safe and can be shared across
/// threads. For this reason the [`Stash`] struct is not wrapped in an [`Arc`],
/// and does not need any self-reference.
///
/// # Design
///
/// The [`Stash`] struct provides a simple and straightforward interface for
/// interacting with the database in an asynchronous manner, abstracting away
/// the details of connection management and query execution and presenting the
/// most common functions through an easy-to-use API, while still allowing for
/// more advanced/direct usage when necessary.
///
/// A key goal is to use available libraries wherever possible and sensible, and
/// to avoid custom implementation of widely-available functionality. This
/// reduces the amount of code that needs to be written and maintained, and
/// reduces the potential for bugs and errors. The [`r2d2`] and [`rusqlite`]
/// crates are used for connection pooling and SQLite database interaction,
/// respectively, and are well-established and widely-used libraries that
/// provide robust and reliable functionality.
///
/// No distinction is made between read and write operations, as this would
/// require additional overhead to detect and enforce, as it would not be
/// possible to rely purely upon method usage, for instance. Therefore, any
/// context of read or write operations is left to the [`rusqlite`] library to
/// handle. It is entirely possible to have a number of read operations running
/// in parallel, and locking is achieved either automatically by SQLite when a
/// write operation is performed, or via the use of a transaction.
///
/// # Interface
///
/// The main usage of the [`Stash`] struct is through the [`query()`][Stash::query()]
/// method, which executes a query on the database and returns the result. The
/// query is passed as a string, and any parameters are passed as a vector of
/// boxed [`ToSql`] trait objects. The function returns a [`Result`] with any
/// rows of data that are returned by the query.
///
/// Notably, it is only possible to compose a [`Statement`](rusqlite::Statement)
/// at the time of execution, and not to prepare it in advance. This is because
/// the [`Statement`](rusqlite::Statement) type is not [`Send`] compatible, and
/// cannot be passed between threads, so cannot cross the async boundary. This
/// is a limitation of the [`rusqlite`] crate, and is not something that can be
/// worked around. There is therefore no possibility of preparing a statement in
/// one action and then executing it in another. However, this is not a
/// significant limitation, as the preparation and execution of a statement are
/// usually done in close proximity, and [`Statement`](rusqlite::Statement)s are
/// fairly quick to create. It is notable, though, that this enforced
/// restriction does result in repeated re-preparation of statements, which
/// could otherwise potentially be done up-front and cached.
///
/// For convenience, an [`execute()`][Stash::execute()] method is also provided,
/// which is very similar to the [`query()`][Stash::query()] method, but does
/// not return any rows of data. Note, however, that this method may be removed
/// in future if it does not prove to be useful in practice.
///
#[derive(Clone)]
pub struct Stash {
    /// A reference-counted pointer to an immutable internal handle, which is
    /// used to identify an individual stash. The handle is an atomic counter,
    /// to manually keep track of the number of instances.
    pub(crate) handle: Arc<()>,

    /// The sender for the stash operations. This is used to send operations to
    /// the worker thread for execution. This is the manner by which the order
    /// of operations is maintained, and how connections are managed and made
    /// thread-safe.
    queue: Arc<QuitOnDrop>,

    /// The [`Watcher`] instance for the [`Stash`], which is used to monitor the
    /// database for changes and notify subscribers. This is used to provide
    /// real-time updates to any subscribers that have registered interest in
    /// changes to the database for given tables.
    watcher: Arc<Watcher>,
}

/// Because the tethered workers are disconnected from the tether and since they also
/// keep the sender queue alive, stash never really terminates.
///
/// This small wrapper ensures that when all references to `Stash` and `Tether` are
/// dropped, we send a signal to the main worker to terminate.
#[derive(Debug)]
struct QuitOnDrop {
    /// Message queue for stash main worker.
    queue: QueueSender<Operation>,
}

impl Drop for QuitOnDrop {
    fn drop(&mut self) {
        drop(self.queue.send(Operation::Quit));
    }
}

impl Deref for QuitOnDrop {
    type Target = QueueSender<Operation>;
    fn deref(&self) -> &Self::Target {
        &self.queue
    }
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
    /// background worker on a separate thread, with a new SQLite connection
    /// pool.
    ///
    /// Note that the pool is created internally by the worker, and fully
    /// managed by it, as there can only be one worker per [`Stash`] instance
    /// and database operations need to be executed sequentially.
    ///
    /// # WARNING
    ///
    /// Please ensure that you handle multiple concurrent connections sensibly,
    /// in order to avoid exhausting the pool. Things like properly waiting for
    /// tasks and threads to complete, for example.
    ///
    /// Be wary in multithreaded environment of possible panics while dealing
    /// with transactions with in-memory storage. There may or may not be a
    /// problem here, caused by [`r2d2_sqlite`]'s connection pool, which in the
    /// past has had similar issues. Though it seems to be patched for
    /// multithreading generally, it might still cause issues for async with
    /// threads, although this is not completely confirmed.
    ///
    ///   - Reference: https://github.com/ivanceras/r2d2-sqlite/issues/39
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
        let (sender, receiver) = flume::unbounded();
        let stash = Self {
            handle: Arc::new(()),
            queue: Arc::new(QuitOnDrop { queue: sender }),
            watcher: Watcher::new().map_err(|e| StashError::WatcherError(e.to_string()))?,
        };
        Worker::start(path, receiver, &stash)?;
        Ok(stash)
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
        let handle = Arc::new(AtomicU32::new(1));
        debug!("Tether ({:p}): Create", Arc::as_ptr(&handle));
        Tether {
            handle,
            queue: Arc::clone(&self.queue),
            start_time: Arc::new(Instant::now()),
            state: Some(State::new()),
            watcher: Arc::clone(&self.watcher),
        }
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
        F: Fn(flume::Sender<()>) -> Box<dyn TableObserver>,
    {
        let (sender, receiver) = flume::unbounded();
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
        let (sender, receiver) = flume::unbounded::<Notification>();
        let operation = Operation::Subscribe(Subscription {
            channel: Some(that_end),
            queue: sender,
            table,
        });
        self.queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))??;
        Ok(receiver)
    }
}

impl Eq for Stash {}

impl PartialEq for Stash {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.handle, &other.handle)
    }
}

/// A handle to a database connection watcher.
#[derive(Debug)]
#[non_exhaustive]
pub struct WatcherHandle {
    /// The receiver for the notifications.
    pub receiver: flume::Receiver<()>,
    /// The handle to stop the watcher.
    pub handle: DropRemoveTableObserverHandle,
}

/// A subscription operation to be executed by the worker.
///
/// This is used for subscribing to [`Notification`]s, such as database change
/// events.
///
/// # See also
///
/// * [`Command`]
/// * [`Information`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`Operation`]
/// * [`Query`]
/// * [`Stash::subscribe()`]
///
struct Subscription {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: Option<OneshotSender<Result<(), StashError>>>,

    /// The queue to which [`Notification`]s will be sent. Note that this is
    /// for *redistributed* notifications — i.e. after the central worker has
    /// received them from the database, it will then send them to all
    /// subscribers, with this being a subscriber-specific queue.
    queue: QueueSender<Notification>,

    /// The table to subscribe to. If [`None`], all tables are subscribed to.
    table: Option<String>,
}

impl OperationLogic for Subscription {
    type Output = ();

    fn channel(&mut self) -> Option<OneshotSender<Result<Self::Output, StashError>>> {
        self.channel.take()
    }

    /// Carries out a subscription.
    ///
    /// **Note: This function does not actually do anything, as the operational
    /// context for subscriptions is the [`Subscription`] instance.**
    ///
    /// # Parameters
    ///
    /// * `connection` - The database connection to use for the operation.
    /// * `stash`      - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// None.
    ///
    fn run(&self, _connection: &AgnosticConnection<'_>) -> Result<(), StashError> {
        Ok(())
    }

    fn start_time(&self) -> Instant {
        // TODO: This may or may not be useful to implement. For now it satisfies
        // TODO: the trait requirements.
        Instant::now()
    }
}

/// Database connection context.
///
/// This struct provides a lightweight, thread-safe database connection context
/// — which is not an actual connection, but a tether to one — that can be
/// shared easily and without concern. It is used to execute queries against the
/// database,
///
/// # Design
///
/// Because [`PooledConnection`] is not [`Send`] compatible, it cannot be passed
/// between threads, and so cannot cross the async boundary. This is an
/// inherited limitation of the [`rusqlite`] crate. The [`Tether`] struct gets
/// around this problem by storing an immutable handle which is used to persist
/// the connection context, and expires when no longer in use. This way, the
/// [`PooledConnection`] remains in the control of the worker, which runs on a
/// dedicated background thread.
///
/// Note that the important thing about a [`Tether`] is the *instance* of the
/// internal handle, and not the actual value of the handle. This is why the
/// handle is simply a unit. It is equivalent to a unique ID, but without having
/// to actually assign a value.
///
/// In addition to the internal handle, the [`Tether`] also stores a reference
/// to the queue, which is used to send queries to the worker for execution.
/// This is the manner by which context is associated.
///
/// When the [`Tether`] is dropped, the reference count to the internal handle
/// decreases, and when there are no remaining strong references, the worker
/// will return the underlying connection to the pool.
///
/// # See also
///
/// * [`Stash::connection()`]
///
pub struct Tether {
    /// A reference-counted pointer to an immutable internal handle, which is
    /// used to identify and specify the associated connection when any database
    /// operations are carried out. The handle is an atomic counter, to manually
    /// keep track of the number of instances.
    handle: Arc<AtomicU32>,

    /// The queue for the [`Worker`] and [`Stash`] to which the [`Tether`] is
    /// associated. This is used to send queries to the worker for execution.
    queue: Arc<QuitOnDrop>,

    /// The time at which the Tether was created.
    start_time: Arc<Instant>,

    /// Watcher instance
    watcher: Arc<Watcher>,

    /// State needed for the connection to be updated on transaction start and
    /// published at the end.
    state: Option<State>,
}

impl Debug for Tether {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tether")
            .field("handle", &self.handle)
            .field("queue", &self.queue)
            .field("start_time", &self.start_time)
            .finish_non_exhaustive()
    }
}

impl Tether {
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
    /// # Transactions, and connection context
    ///
    /// The [`Interface`] trait is implemented for [`Stash`], [`Tether`], and
    /// [`AgnosticInterface`].
    ///
    ///   - If run against a [`Stash`] instance, the query will be executed
    ///     against a new database connection created specifically for its use.
    ///     For once-off, unrelated queries this is fine, but when using
    ///     transactions it is critical to run all related queries against the
    ///     same connection, in which case use [`Tether::execute()`].
    ///
    ///   - If run against a [`Tether`] instance, the query will be executed in
    ///     context to that instance, against the associated database
    ///     connection.
    ///
    /// # Mechanism
    ///
    /// To be technically accurate, this function does not actually execute the
    /// query, but provides an interface to do so. It adds the query to the
    /// database operations queue, where it will be picked up and processed by
    /// the background worker, and the result returned.
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
    ///
    /// Note that, unlike the [`query()`][Tether::query()] method, no
    /// distinction is made between execution and preparation errors.
    ///
    /// # See also
    ///
    /// * [`Interface::query()`]
    /// * [`params!`](crate::utils::params)
    ///
    pub async fn execute<Q: Into<String> + Send>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<usize, StashError> {
        perform_execute(&self.queue, query, params, Some(Arc::clone(&self.handle))).await
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
    ///
    /// # See also
    ///
    /// * [`Model::load()`]
    ///
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
    /// # Transactions, and connection context
    ///
    /// The [`Interface`] trait is implemented for [`Stash`], [`Tether`], and
    /// [`AgnosticInterface`].
    ///
    ///   - If run against a [`Stash`] instance, the query will be executed
    ///     against a new database connection created specifically for its use.
    ///     For once-off, unrelated queries this is fine, but when using
    ///     transactions it is critical to run all related queries against the
    ///     same connection, in which case use [`Tether::execute()`].
    ///
    ///   - If run against a [`Tether`] instance, the query will be executed in
    ///     context to that instance, against the associated database
    ///     connection.
    ///
    /// # Mechanism
    ///
    /// To be technically accurate, this function does not actually execute the
    /// query, but provides an interface to do so. It adds the query to the
    /// database operations queue, where it will be picked up and processed by
    /// the background worker, and the result returned.
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
    ///   - [`DowncastError`](StashError::DowncastError) - Problem downcasting
    ///     from [`Any`](std::any::Any) to `T`.
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
        perform_query(&self.queue, query, params, Some(Arc::clone(&self.handle))).await
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
        perform_value_query(&self.queue, query, params, Some(Arc::clone(&self.handle))).await
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
    /// See [`Interface::query_values()`] for more information.
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

    /// The transaction will no produce any notifications.
    ///
    /// This method is used to start a transaction without listening for changes.
    /// It is needed for internal implementation of the watch mechanism and scrollers.
    ///
    pub async fn quiet_transaction(&mut self) -> Result<Bond<'_>, StashError> {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::StartTransaction(Command::new(
            Some(that_end),
            Some(Arc::clone(&self.handle)),
        ));
        self.queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))??;

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

    /// Subscribes to notifications of changes to a specific table.
    ///
    /// # Errors
    ///
    /// See [`Stash::subscribe()`].
    pub fn subscribe_to<F>(&self, observer: F) -> Result<WatcherHandle, StashError>
    where
        F: Fn(flume::Sender<()>) -> Box<dyn TableObserver>,
    {
        let (sender, receiver) = flume::unbounded();
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

impl Drop for Tether {
    fn drop(&mut self) {
        if self
            .queue
            .send(Operation::CloseConnection(Command::new(
                None,
                Some(Arc::clone(&self.handle)),
            )))
            .is_err()
        {
            error!("Failed to send CloseConnection operation to tethered queue");
        }
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

    #[allow(clippy::mem_forget)]
    /// Internal commit implementation.
    ///
    /// This method is used to commit a transaction without publishing changes.
    /// It is needed for internal implementation of the watch mechanism.
    ///
    async fn commit_(self, publish_changes: bool) -> Result<(), StashError> {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::CommitTransaction(Command::new(
            Some(that_end),
            Some(Arc::clone(&self.tether.handle)),
        ));
        self.tether
            .queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))??;

        if publish_changes {
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
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::RollbackTransaction(Command::new(
            Some(that_end),
            Some(Arc::clone(&self.tether.handle)),
        ));
        self.tether
            .queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))??;

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
        if self
            .queue
            .send(Operation::RollbackTransaction(Command::new(
                None,
                Some(Arc::clone(&self.tether.handle)),
            )))
            .is_err()
        {
            error!("Failed to send RollbackTransaction operation to tethered queue on transaction drop");
        }
    }
}

/// Connection-specific worker for executing queries.
///
/// This struct provides a "tethered", i.e. connection-specific, worker for
/// executing queries. It carries out database operations related to its
/// established connection in a separate thread. It receives its instructions
/// from the main worker via a dedicated queue, and sends the results back to
/// the original caller via a oneshot channel.
///
/// There is no `new()` method for this struct, as it is created internally when
/// a new tethered worker thread is started.
///
/// Notably, everything the tethered worker does is synchronous — it does not
/// use async at all.
///
#[derive(Debug)]
struct TetheredWorker {
    /// A reference-counted pointer to an immutable internal handle, which is
    /// used to identify and specify the associated database connection. The
    /// handle is an atomic counter, to manually keep track of the number of
    /// instances. It is stored here as a weak reference to the connection
    /// handle, so that the connection can be re-used if it is already
    /// registered, but also removed from the list if it is no longer in use.
    conn_handle: Weak<AtomicU32>,

    /// The sender side of the tethered worker's queue.
    queue: QueueSender<Operation>,

    /// The join handle for the thread in which the tethered worker runs.
    thread_handle: Option<JoinHandle<()>>,
}

impl TetheredWorker {
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
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    // This is infallible in this location
    #[allow(clippy::unwrap_in_result)]
    #[allow(clippy::unwrap_used)]
    fn handle_operation<'tx>(
        operation: Operation,
        connection: &'tx PooledConnection<SqliteConnectionManager>,
        mut transaction: Option<Transaction<'tx>>,
        queue: &QueueSender<Operation>,
    ) -> Option<Transaction<'tx>> {
        match operation {
            Operation::CloseConnection(_) => {
                // This is a special case, which is handled outside of this function. We
                // should never get here. If we do, it means there is an error in the logic
                // of this module. Note that we cannot return an error to the original
                // caller, as there is no oneshot channel for notifications, plus the
                // context would not make any sense.
                warn!("Unexpectedly reached CloseConnection variant in TetheredWorker::handle_operation()");
            }
            Operation::CommitTransaction(mut command) => {
                if let Some(conn_handle) = command.conn_handle.clone() {
                    debug!(
                        "Tether ({:p}): CommitTransaction Command (id: {}, waiting: {}µs)",
                        Arc::as_ptr(&conn_handle),
                        command.id,
                        command.start_time().elapsed().as_micros(),
                    );
                    if let Some(tx) = transaction.take() {
                        command.send_back(tx.commit().map_err(StashError::TransactionError));
                        // Notify the main worker that the transaction has been committed
                        if queue
                            .send(Operation::NotifyCommitTransaction(Arc::clone(&conn_handle)))
                            .is_err()
                        {
                            error!(
                                "Failed to send NotifyCommitTransaction operation to main queue"
                            );
                        }
                    } else {
                        command.send_back(Err(StashError::NoActiveTransaction));
                    }
                } else {
                    command.send_back(Err(StashError::TransactionCommandWithoutTether));
                }
            }
            Operation::Instruct(mut instruction) => {
                debug!(
                    "Tether ({:p}): Instruction to execute (id: {}, waiting: {}µs)",
                    instruction.conn_handle.as_ref().map_or(null(), Arc::as_ptr),
                    instruction.id,
                    instruction.start_time().elapsed().as_micros(),
                );
                // Note: The query count got incremented when the Instruction was created.
                instruction.send_back(instruction.run(&transaction.as_ref().map_or(
                    AgnosticConnection::Unbound(connection),
                    AgnosticConnection::Engaged,
                )));
            }
            Operation::Publish(_) => {
                // Technically, these cannot occur here, as subscription operations are
                // global in scope and not connection-specific. We should never get here. If
                // we do, it means there is an error in the logic of this module. Note that
                // we cannot return an error to the original caller, as there is no oneshot
                // channel for notifications, plus the context would not make any sense.
                warn!("Unexpectedly reached Publish variant in TetheredWorker::handle_operation()");
            }
            Operation::Query(mut query) => {
                debug!(
                    "Tether ({:p}): Query to run (id: {}, waiting: {}µs)",
                    query.conn_handle.as_ref().map_or(null(), Arc::as_ptr),
                    query.id,
                    query.start_time().elapsed().as_micros(),
                );
                // Note: The query count got incremented when the Query was created.
                query.send_back(query.run(&transaction.as_ref().map_or(
                    AgnosticConnection::Unbound(connection),
                    AgnosticConnection::Engaged,
                )));
            }
            Operation::RollbackTransaction(mut command) => {
                if let Some(conn_handle) = command.conn_handle.clone() {
                    debug!(
                        "Tether ({:p}): RollbackTransaction Command (id: {}, waiting: {}µs)",
                        Arc::as_ptr(&conn_handle),
                        command.id,
                        command.start_time().elapsed().as_micros(),
                    );
                    if let Some(tx) = transaction.take() {
                        command.send_back(tx.rollback().map_err(StashError::TransactionError));
                        // Notify the main worker that the transaction has been rolled back.
                        if queue
                            .send(Operation::NotifyRollbackTransaction(Arc::clone(
                                &conn_handle,
                            )))
                            .is_err()
                        {
                            error!(
                                "Failed to send NotifyRollbackTransaction operation to main queue"
                            );
                        }
                    } else {
                        command.send_back(Err(StashError::NoActiveTransaction));
                    }
                } else {
                    command.send_back(Err(StashError::TransactionCommandWithoutTether));
                }
            }
            Operation::StartTransaction(mut command) => {
                if let Some(conn_handle) = command.conn_handle.clone() {
                    debug!(
                        "Tether ({:p}): StartTransaction Command (id: {}, waiting: {}µs)",
                        command.conn_handle.as_ref().map_or(null(), Arc::as_ptr),
                        command.id,
                        command.start_time().elapsed().as_micros(),
                    );
                    if transaction.is_none() {
                        // We call new_unchecked() here because new() requires a mutable borrow.
                        // Being unchecked does not matter, as we perform the necessary checks
                        // ourselves.
                        match Transaction::new_unchecked(
                            connection,
                            // This is not well-documented, but is significant. The behaviour mode of
                            // the transaction affects when a lock is acquired - this part is obvious
                            // and IS documented. For reference:
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
                        .map_err(StashError::ExecutionError)
                        {
                            Ok(tx) => {
                                transaction = Some(tx);
                                command.send_back(Ok(()));
                                // Notify the main worker that a transaction has started.
                                if queue
                                    .send(Operation::NotifyStartTransaction(conn_handle))
                                    .is_err()
                                {
                                    error!(
                                        "Failed to send NotifyStartTransaction operation to main queue"
                                    );
                                }
                            }
                            Err(error) => {
                                command.send_back(Err(error));
                            }
                        };
                    } else {
                        command.send_back(Err(StashError::TransactionAlreadyStarted));
                    }
                } else {
                    command.send_back(Err(StashError::TransactionCommandWithoutTether));
                }
            }
            Operation::Subscribe(mut subscription) => {
                // Technically, these cannot occur here, as subscription operations are
                // global in scope and not connection-specific. We should never get here. If
                // we do, it means there is an error in the logic of this module.
                subscription.send_back(Err(StashError::SubscriptionError));
            }

            Operation::Quit
            | Operation::NotifyCommitTransaction(_)
            | Operation::NotifyRollbackTransaction(_)
            | Operation::NotifyStartTransaction(_) => {
                // These should never occur in the tether work. If they do
                // it's a bug.
                warn!("Received unexpected transaction notification");
            }
        }

        transaction
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
    fn start(
        conn_handle: Weak<AtomicU32>,
        pool: Pool<SqliteConnectionManager>,
        queue: QueueSender<Operation>,
    ) -> Self {
        let conn_handle_clone = Weak::clone(&conn_handle);
        let (sender, receiver) = flume::unbounded::<Operation>();

        // Spawn a thread to run the worker. This thread will execute the queries
        // sequentially, as they are received, on a persistent connection, and will
        // return the results to the original caller via oneshot channels.
        #[allow(clippy::cognitive_complexity)]
        let thread_handle = spawn(move || {
            #[allow(unused_assignments)]
            let mut connection: Option<PooledConnection<SqliteConnectionManager>> = None;
            let mut transaction: Option<Transaction<'_>> = None;

            // The first time an operation is received, we attempt to acquire a database
            // connection from the pool. This is done lazily so that there is no delay
            // in creating [`Tether`] instances, and so that any errors can be returned
            // to the caller. It is important that this happens just once, ahead of the
            // main loop starting, to avoid borrowing issues (because when transactions
            // are started, they borrow the underlying connection).
            #[allow(clippy::unwrap_used)]
            if let Ok(mut operation) = receiver.recv() {
                if let Operation::CloseConnection(_) = operation {
                    return;
                }
                connection = match pool.get_and_subscribe(queue.clone(), Some(conn_handle_clone)) {
                    Ok(conn) => Some(conn),
                    Err(error) => {
                        operation.send_back_error(error);
                        return;
                    }
                };
                // Set WAL mode
                drop(
                    connection
                        .as_ref()
                        .unwrap()
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
                        .inspect_err(|err| {
                            error!("Failed to set configuration on connection: {:?}", err);
                        }),
                );
                let conn = connection.as_mut().unwrap();
                drop(State::start_tracking(&mut **conn).inspect_err(|err| {
                    error!("Failed to set watcher on the connection: {:?}", err);
                }));

                transaction = Self::handle_operation(
                    operation,
                    connection.as_ref().unwrap(),
                    transaction,
                    &queue,
                );
            } else {
                return;
            }

            #[allow(clippy::unwrap_used)]
            while let Ok(operation) = receiver.recv() {
                if matches!(operation, Operation::Quit) {
                    return;
                }
                // Ownership of the transaction is sent and returned to avoid borrowing
                // issues - otherwise the borrow checker believes the borrow is still active
                // on the next loop.
                if let Operation::CloseConnection(command) = operation {
                    debug!(
                        "Tether ({:p}): Close connection",
                        command.conn_handle.as_ref().map_or(null(), Arc::as_ptr)
                    );
                    if let Some(tx) = transaction.take() {
                        if tx.rollback().is_err() {
                            error!("Failed to roll back transaction upon connection closure");
                        }
                        // Notify the main worker that the transaction has been rolled back.
                        let Some(handle) = command.conn_handle else {
                            error!("Closing connection without a handle, can not send NotifyRollbackNotification to main queue");
                            return;
                        };
                        if queue
                            .send(Operation::NotifyRollbackTransaction(handle))
                            .is_err()
                        {
                            error!(
                                "Failed to send NotifyRollbackTransaction operation to main queue"
                            );
                        }
                    }
                    return;
                }
                transaction = Self::handle_operation(
                    operation,
                    connection.as_ref().unwrap(),
                    transaction,
                    &queue,
                );
            }
        });

        Self {
            conn_handle,
            queue: sender,
            thread_handle: Some(thread_handle),
        }
    }
}

impl Drop for TetheredWorker {
    fn drop(&mut self) {
        if let Some(handle) = self.conn_handle.upgrade() {
            if self
                .queue
                .send(Operation::CloseConnection(Command::new(
                    None,
                    Some(Arc::clone(&handle)),
                )))
                .is_err()
            {
                error!(
                    "{:p} Failed to send CloseConnection operation to tethered queue",
                    Arc::as_ptr(&handle)
                );
            }
        }

        if let Some(thread_handle) = self.thread_handle.take() {
            // Wait for the thread to complete its work
            if let Err(err) = thread_handle.join() {
                error!("Failed to join tethered worker thread: {:?}", err);
            }
        }
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
    notifications_buffer: HashMap<*const AtomicU32, Vec<Notification>>,

    /// A pool of SQLite connections. Although the pool itself is thread-safe,
    /// being `Pool<M>(Arc<SharedPool<M>>)` underneath, the connections are not.
    /// Therefore we store the pool centrally on the worker, keep the created
    /// connections on the worker, and issue thread-safe [`Tether`]s to the
    /// caller.
    pool: Pool<SqliteConnectionManager>,

    /// The sender side of the main worker's queue.
    queue: QueueSender<Operation>,

    /// The runtime for the worker. This is used to spawn async tasks
    /// independently of the main application runtime.
    runtime: Runtime,

    /// The list of subscribers to the stash. This is used to send notifications
    /// whenever changes are made to the database.
    subscribers: Vec<(QueueSender<Notification>, Option<String>)>,

    /// A map of active connections with their tethered workers. This is used to
    /// keep track of the connections that are currently in use, and to
    /// associate them with the [`Tether`]s that are issued to the caller.
    /// Persistent connections are handled through dedicated workers on their
    /// own threads, with their own messaging queues. These "tethered" workers
    /// create [`PooledConnection`]s, which are not thread-safe, and so are not
    /// directly accessible by the caller. The join handle for the thread is
    /// stored in the [`TetheredWorker`] instance, along with the sender side of
    /// the tethered worker's queue.
    ///
    /// A weak reference to the connection handle is also stored, so that the
    /// connection can be re-used if it is already registered, but also removed
    /// from the list if it is no longer in use.
    ///
    /// Note that the key is the *pointer* to the weak reference, and not the
    /// actual weak reference itself. This is because a `Weak<AtomicU32>` cannot
    /// be a [`HashMap`] key. Use of a pointer here is safe, as the pointer is
    /// unique to the connection, and is only used for the purpose of
    /// identification.
    ///
    /// The association between an actual database connection instance, i.e. a
    /// [`PooledConnection`], and a usage reference, i.e. a [`Tether`], is made
    /// with a weak reference to a unit, in context to a thread. The strong
    /// reference, i.e. the [`Arc`] wrapping the unit, is given out, and when it
    /// is no longer used the [`Weak`] stored in the [`TetheredWorker`] will
    /// expire, which can be detected and the connection removed.
    ///
    /// This approach has the minor downside of require a garbage-collection
    /// cycle, but the major upside of avoiding the need to formally issue and
    /// check connection IDs, which would require error handling and also expose
    /// the risk of the wrong ID being used. By sharing a reference-counted
    /// pointer there is no way of side-stepping the association, as the issued
    /// [`Tether`] is bound to the matching [`TetheredWorker`]. The clean-up is
    /// extremely quick and can happen at suitable intervals, only having to
    /// check the [`Weak`] pointers and removing any expired connections.
    tethers: HashMap<*const AtomicU32, TetheredWorker>,
}

impl Worker {
    /// Handles a database operation.
    ///
    /// This function processes a database operation that the main worker has
    /// received from its queue, executing the necessary logic against the
    /// database connection, and returning the result to the original caller. It
    /// is the core logic of the worker thread, and is responsible for managing
    /// the connection and transaction state, and executing the queries.
    ///
    /// It has a fundamental goal of being as quick and lightweight as possible,
    /// so that it doesn't hold up the main worker thread that called it.
    ///
    /// # Parameters
    ///
    /// * `operation` - The database operation to handle.
    ///
    /// # Errors
    ///
    /// If there is a problem obtaining a connection from the pool then the
    /// error will be returned to the original caller via the oneshot channel.
    /// As it's not possible to continue in this situation, the function
    /// returns. The actual [`StashError::TetherError`] is not returned, as it
    /// has been sent to the original sender, and is not cloneable. This is
    /// okay, as the function calling this one cannot do anything about it
    /// anyway.
    ///
    /// If there is a problem spawning the blocking task to carry out the
    /// operation, then the error cannot be returned to the original caller, as
    /// the operation has by this time been unpacked and sent into the blocking
    /// thread, so we no longer have it. In this case we could return some kind
    /// of [`StashError`] variant, but the calling function would not be able to
    /// actually do anything about it other than log it, plus we would need to
    /// differentiate between this situation and that of the connection error.
    /// Therefore we handle this error as best we can by logging it, and so
    /// there is no current need to return any error as they are already dealt
    /// with.
    ///
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    fn handle_operation(&mut self, operation: Operation) {
        let pool = self.pool.clone();
        let queue = self.queue.clone();
        match operation {
            Operation::CloseConnection(_) => {
                // At present this can only be sent directly to the tethered worker's queue.
                // We should never get here. If we do, it means there is an error in the
                // logic of this module. Note that we cannot return an error to the original
                // caller, as there is no oneshot channel for notifications, plus the
                // context would not make any sense.
                warn!("Unexpectedly reached CloseConnection variant in Worker::handle_operation()");
            }
            Operation::NotifyCommitTransaction(conn_handle) => {
                let handle = Arc::as_ptr(&conn_handle);
                debug!(
                    "Stash: Publishing deferred Notification list for committed transaction ({:p})",
                    handle
                );
                if let Some(notifications) = self.notifications_buffer.remove(&handle) {
                    // This is a slight trade-off - it's better to spend a small amount of time
                    // cloning the subscribers list (which is cheap) than to block the main
                    // thread while we loop through them. This way, we can offload the sending
                    // as an async task, plus the subscriber list is a safe snapshot from this
                    // point in time.
                    //TODO(ET-1400) - Proper unsubscribe support
                    let subscribers = self.subscribers.clone();
                    let debug_string = format!(
                        "Stash: Publishing {} from Tether {:p}",
                        notifications.len(),
                        handle
                    );
                    drop(self.runtime.spawn(async move {
                        debug!("{}", debug_string);
                        for notification in notifications {
                            #[allow(clippy::pattern_type_mismatch)]
                            for (subscriber, table) in &subscribers {
                                if table.as_ref().is_none_or(|t| t == &notification.table) {
                                    drop(subscriber.send_async(notification.clone()).await);
                                }
                            }
                        }
                    }));
                } else {
                    // In theory this should never happen, but we also can't do anything with it
                    error!(
                        "Queue error: Failed to obtain Notification list for committed transaction"
                    );
                }
            }
            Operation::Instruct(mut instruction) => {
                debug!(
                    "Stash (ad-hoc conn): Instruction to execute (id: {}, waiting: {}µs)",
                    instruction.id,
                    instruction.start_time().elapsed().as_micros(),
                );
                drop(self.runtime.spawn(async move {
                    match pool.get_and_subscribe(queue, None) {
                        Ok(connection) => {
                            // Spawn a blocking task to execute the query. This is necessary because
                            // rusqlite is synchronous, so we need to tell the Tokio runtime that
                            // this task will block.
                            spawn_blocking(move || {
                                // Note: The query count got incremented when the Instruction was created.
                                instruction.send_back(
                                    instruction.run(&AgnosticConnection::Unbound(&connection)),
                                );
                            })
                            .await
                            .unwrap_or_else(|err| {
                                // In theory this should never happen, but we also can't do anything with it
                                error!("Thread error: Failed to spawn blocking task: {err:?}");
                            });
                        }
                        Err(err) => instruction.send_back(Err(err)),
                    }
                }));
            }
            Operation::Publish(notification) => {
                let handle = notification
                    .conn_handle
                    .as_ref()
                    .map_or(null(), Arc::as_ptr);
                if let Some(notifications) = self.notifications_buffer.get_mut(&handle) {
                    debug!(
                        "Stash: Notification to publish (deferring, tether={:p})",
                        handle
                    );
                    notifications.push(notification);
                    return;
                }
                debug!("Stash: Notification to publish");
                // This is a slight trade-off - it's better to spend a small amount of time
                // cloning the subscribers list (which is cheap) than to block the main
                // thread while we loop through them. This way, we can offload the sending
                // as an async task, plus the subscriber list is a safe snapshot from this
                // point in time.

                // Remove any subscribers that have perished.
                // TODO(ET-1400): Proper unsubscribe API.
                #[allow(clippy::pattern_type_mismatch)]
                self.subscribers.retain(|(s, _)| !s.is_disconnected());

                let subscribers = self.subscribers.clone();
                drop(self.runtime.spawn(async move {
                    for (subscriber, table) in subscribers {
                        if table.as_ref().is_none_or(|t| t == &notification.table) {
                            // Because there is no way to unsubscribe right now
                            // this can fail very frequently. We used to log the
                            // errors here, but that can lead to log spam.
                            drop(subscriber.send_async(notification.clone()).await);
                        }
                    }
                }));
            }
            Operation::Query(mut query) => {
                debug!(
                    "Stash (ad-hoc conn): Query to run (id: {}, waiting: {}µs)",
                    query.id,
                    query.start_time().elapsed().as_micros(),
                );
                drop(self.runtime.spawn(async move {
                    match pool.get_and_subscribe(queue, None) {
                        Ok(connection) => {
                            // Spawn a blocking task to execute the query. This is necessary because
                            // rusqlite is synchronous, so we need to tell the Tokio runtime that
                            // this task will block.
                            spawn_blocking(move || {
                                // Note: The query count got incremented when the Query was created.
                                query.send_back(
                                    query.run(&AgnosticConnection::Unbound(&connection)),
                                );
                            })
                            .await
                            .unwrap_or_else(|err| {
                                // In theory this should never happen, but we also can't do anything with it
                                error!("Thread error: Failed to spawn blocking task: {err:?}");
                            });
                        }
                        Err(err) => query.send_back(Err(err)),
                    }
                }));
            }
            Operation::NotifyRollbackTransaction(conn_handle) => {
                debug!("Stash: Clearing deferred Notification list for aborted transaction");
                drop(self.notifications_buffer.remove(&Arc::as_ptr(&conn_handle)));
            }
            Operation::NotifyStartTransaction(conn_handle) => {
                debug!("Stash: Initializing deferred Notification list for transaction");
                drop(
                    self.notifications_buffer
                        .insert(Arc::as_ptr(&conn_handle), vec![]),
                );
            }
            Operation::Subscribe(mut subscription) => {
                debug!("Stash: Subscription request");

                let sub_queue = subscription.queue.clone();
                let sub_table = subscription.table.clone();
                self.subscribers.push((sub_queue, sub_table));

                // Although this operation is infallible, a response still needs to be sent,
                // as the caller might be waiting on the oneshot channel in order to
                // continue.
                subscription.send_back(Ok(()));
            }

            Operation::Quit
            | Operation::StartTransaction(_)
            | Operation::CommitTransaction(_)
            | Operation::RollbackTransaction(_) => {
                // These should not be handled by the main worker. If it
                // happens it means there is a bug.
                warn!("Received unexpected transaction command");
            }
        };
    }

    /// Prunes the list of tethers, removing any that are no longer in use.
    ///
    /// This is a garbage-collection function, that iterates over the list of
    /// tethers, and removes any that are no longer in use. This is determined
    /// by checking the strong count of the weak reference. If the strong count
    /// is zero, it means that all uses have been dropped, meaning the
    /// connection is no longer in use, and so the tether can be removed.
    ///
    fn prune_tethers(&mut self) {
        self.tethers
            .retain(|_, worker| worker.conn_handle.strong_count() > 0);
    }

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
    fn start(
        path: Option<&Path>,
        receiver: QueueReceiver<Operation>,
        stash: &Stash,
    ) -> Result<(), StashError> {
        #[allow(clippy::single_match_else)]
        match path {
            Some(p) => debug!("New Stash with file: {:?}", p),
            None => debug!("New Stash with in-memory database"),
        }
        let manager = path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );
        let pool = Pool::builder()
            .max_size(MAX_CONNECTIONS)
            .build(manager)
            .map_err(StashError::TetherError)?;

        let queue = stash.queue.queue.clone();

        // Spawn a thread to run the worker. This thread will execute the queries
        // sequentially, as they are received, and will return the results via
        // oneshot channels.
        #[allow(clippy::cognitive_complexity)]
        drop(spawn(move || {
            let runtime = match Runtime::new() {
                Ok(runtime) => runtime,
                Err(err) => {
                    error!("Thread error: Failed to create Tokio runtime: {err}");
                    return;
                }
            };
            let mut worker = Self {
                notifications_buffer: HashMap::new(),
                pool,
                queue,
                runtime,
                subscribers: Vec::new(),
                tethers: HashMap::new(),
            };

            while let Ok(operation) = receiver.recv() {
                let mut is_connection_close = false;
                let conn_handle = match operation {
                    Operation::CloseConnection(ref command) => {
                        is_connection_close = true;
                        command.conn_handle.clone()
                    }
                    Operation::CommitTransaction(ref command)
                    | Operation::RollbackTransaction(ref command)
                    | Operation::StartTransaction(ref command) => command.conn_handle.clone(),
                    Operation::Instruct(ref instruction) => instruction.conn_handle.clone(),
                    Operation::Publish(_)
                    | Operation::Subscribe(_)
                    | Operation::NotifyCommitTransaction(_)
                    | Operation::NotifyRollbackTransaction(_)
                    | Operation::NotifyStartTransaction(_) => None,
                    Operation::Query(ref query) => query.conn_handle.clone(),
                    Operation::Quit => {
                        // Force terminate all tether workers
                        #[allow(clippy::iter_over_hash_type)]
                        for tether in worker.tethers.values() {
                            drop(tether.queue.send(Operation::Quit));
                        }
                        debug!("Stash: Quit request");
                        return;
                    }
                };
                match conn_handle {
                    // If a tethered connection handle was specified, it means that this query
                    // is part of a set of related queries which need to be executed against the
                    // same connection — such as when using transactions. These related queries
                    // will be carried out on a sync thread, which is not managed by Tokio. If
                    // this thread has already been spawned, it will be re-used; otherwise a new
                    // one will be created and registered. This thread persistence is necessary
                    // in order to maintain the not-thread-safe PooledConnection context across
                    // the related queries, while allowing calling code to be fully async.
                    Some(handle) => {
                        let (_, tethered_worker) = worker.get_tethered_worker(&handle);
                        if tethered_worker.queue.send(operation).is_err() {
                            // In this situation, we cannot send an error back to the caller, as the
                            // oneshot channel was sent to the queue, and is no longer available. This
                            // situation should never occur in reality, as the queue is unbounded, and
                            // so should never be full. Additionally, the dedicated worker thread should
                            // remain alive until we terminate it.
                            error!(
                                "Queue error: Failed sending message to connection-specific worker"
                            );
                        }
                    }
                    // If no tethered connection handle was specified, it means that this is a
                    // once-off query, and so it will be carried out on a new async thread,
                    // managed by the Tokio runtime.
                    None => {
                        worker.handle_operation(operation);
                    }
                }

                // Run garbage collection
                if is_connection_close {
                    worker.prune_tethers();
                    debug!(
                        "Garbage collection finished: {} registered tethers",
                        worker.tethers.len()
                    );
                }
            }
        }));

        Ok(())
    }

    /// Gets a connection-specific worker from the pool.
    ///
    /// This function gets a connection-specific, i.e. "tethered", worker from
    /// the pool, or creates one and registers it for re-use.
    ///
    /// The internal list of associated [`Tether`] connection handles is checked
    /// to see if the connection-specific worker is already registered. If it
    /// is, the existing worker's queue sender is returned. If it is not, a new
    /// tethered worker is created with a dedicated sync thread and registered,
    /// and its queue sender returned. A registration is made by storing a weak
    /// reference to the connection handle supplied from the [`Tether`]
    /// instance, against the join handle for the connection-specific worker's
    /// thread and queue sender.
    ///
    /// If the specified connection handle is not already registered then it
    /// means that this is a new connection request, as the process of
    /// requesting a new connection is disassociated from the actual acquisition
    /// of the connection itself. This is because the connection is only created
    /// when the first query is executed, and so the [`Tether`] is created and
    /// returned immediately, with no delay. Note that the connection-specific
    /// worker will not actually acquire a connection until it receives its
    /// first query.
    ///
    /// The connection will be returned to the pool by garbage collection once
    /// the [`Tether`] goes out of scope, as the strong reference will expire.
    ///
    /// # Parameters
    ///
    /// * `conn_handle` - The handle of the connection to use for the queries. A
    ///                   connection-specific worker in its own dedicated thread
    ///                   will be created and associated if not already
    ///                   registered, and re-used otherwise.
    ///
    /// # Returns
    ///
    /// A tuple containing a boolean indicating whether the worker was created
    /// by this call, and the worker.
    ///
    /// # See also
    ///
    /// * [`Stash::connection()`]
    /// * [`Tether`]
    /// * [`TetheredWorker::start()`]
    ///
    fn get_tethered_worker(&mut self, conn_handle: &Arc<AtomicU32>) -> (bool, &TetheredWorker) {
        let weak_ref = Arc::downgrade(conn_handle);
        // This code uses the Entry API to avoid double mutable borrow of self.
        match self.tethers.entry(weak_ref.as_ptr()) {
            Entry::Occupied(entry) => (false, entry.into_mut()),
            Entry::Vacant(entry) => (
                true,
                entry.insert(TetheredWorker::start(
                    weak_ref,
                    self.pool.clone(),
                    self.queue.clone(),
                )),
            ),
        }
    }
}

/// Logic for carrying out an operation on the database.
///
/// This trait provides the interface for providing and running logic to carry
/// out an operation on the database.
///
/// # See also
///
/// * [`Command`]
/// * [`Instruction`]
/// * [`Notification`]
/// * [`Operation`]
/// * [`Query`]
/// * [`Subscription`]
///
trait OperationLogic {
    /// The type of the output of the operation, i.e. what is returned by the
    /// [`run()`](OperationLogic::run()) method's implementation.
    type Output;

    /// The oneshot channel used to send the result back to the caller.
    fn channel(&mut self) -> Option<OneshotSender<Result<Self::Output, StashError>>>;

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

    /// Carries out an operation on the database.
    ///
    /// This function carries out, or runs, an operation on the database. Its
    /// exact behaviour is determined by the implementation of the trait, and
    /// the associated documentation should be consulted for more details.
    ///
    /// # Parameters
    ///
    /// * `connection` - The database connection to use for the operation.
    /// * `stash`      - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// Various [`StashError`] variants can be returned. For more details see
    /// the individual implementations of this trait.
    ///
    /// # See also
    ///
    /// * [`Instruction`]
    /// * [`Operation`]
    /// * [`Query`]
    ///
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<Self::Output, StashError>;

    /// Sends the result back to the caller.
    ///
    /// This function sends the result back to the caller via the oneshot
    /// channel. If this fails, an error is logged. No error is returned from
    /// this function because there's not anything that can actually be done
    /// about it.
    ///
    /// # Parameters
    ///
    /// * `result` - The result to send back to the caller.
    /// * `stash`  - The associated [`Stash`] instance for the operation.
    ///
    #[allow(clippy::used_underscore_binding)]
    fn send_back(&mut self, result: Result<Self::Output, StashError>) {
        if let Some(channel) = self.channel().take() {
            // If sending down the oneshot channel fails, send() returns the message to
            // us. It's not particularly interesting what that message is, as we never
            // expect this to fail, so we just log the error event.
            if channel.send(result).is_err() {
                error!("Oneshot error: Failed sending result back to caller");
            }
        } else {
            error!("Oneshot error: Sender already used");
        }
    }

    /// When the operation was started, i.e. created.
    fn start_time(&self) -> Instant;
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
        queue: QueueSender<Operation>,
        conn_handle: Option<Weak<AtomicU32>>,
    ) -> Result<PooledConnection<M>, StashError>;
}

impl PoolExt<SqliteConnectionManager> for Pool<SqliteConnectionManager> {
    fn get_and_subscribe(
        &self,
        queue: QueueSender<Operation>,
        conn_handle: Option<Weak<AtomicU32>>,
    ) -> Result<PooledConnection<SqliteConnectionManager>, StashError> {
        let connection = self.get().map_err(StashError::TetherError)?;
        connection.update_hook(Some(
            move |action: Action, _db_name: &str, table_name: &str, row_id: i64| {
                let conn_handle_clone = match conn_handle {
                    #[allow(clippy::single_match_else)]
                    Some(ref weak_handle) => match weak_handle.upgrade() {
                        Some(handle) => Some(handle),
                        None => {
                            error!("Queue error: Failed to upgrade connection handle");
                            return;
                        }
                    },
                    None => None,
                };
                #[allow(clippy::cast_sign_loss)]
                if queue
                    .send(Operation::Publish(Notification {
                        conn_handle: conn_handle_clone,
                        action,
                        table: table_name.to_owned(),
                        row: row_id as u64,
                    }))
                    .is_err()
                {
                    error!("Queue error: Failed to publish a Notification to the worker thread");
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

/// Runs a query and returns the affected row count.
///
/// This function prepares a query and executes it on the database, and then
/// indicates whether it was successful, returning the number of affected rows.
/// It is the internal function that actually does the querying for the public
/// interface methods [`Stash::execute()`] and [`Tether::execute()`].
///
/// For full usage details, see [`Stash::execute()`].
///
/// # Parameters
///
/// * `stash`       - The [`Stash`] instance to use for the query.
/// * `query`       - The query to execute.
/// * `params`      - The parameters to pass to the query.
/// * `conn_handle` - The handle of the connection to use for the query.
///
/// # Errors
///
/// See [`Stash::execute()`].
///
/// # See also
///
/// * [`Stash::execute()`]
/// * [`Tether::query()`]
/// * [`params!`](crate::utils::params)
///
async fn perform_execute<Q: Into<String> + Send>(
    queue: &QueueSender<Operation>,
    query: Q,
    params: Vec<Box<dyn ToSql + Send>>,
    conn_handle: Option<Arc<AtomicU32>>,
) -> Result<usize, StashError> {
    let (that_end, this_end) = oneshot::channel();
    let operation = Operation::Instruct(Instruction::new(
        Some(that_end),
        conn_handle,
        query.into(),
        params,
    ));
    queue
        .send_async(operation)
        .await
        .map_err(|err| StashError::QueueError(err.to_string()))?;
    this_end
        .await
        .map_err(|err| StashError::OneShotError(err.to_string()))?
}

/// Runs a query and returns rows with a singular value.
///
/// This function prepares a query and executes it on the database, and returns
/// the resulting rows of data as a collection of instances of the specified `T`
/// type, where `T` is any single type implementing the [`FromSql`] and
/// [`ToSql`] trait. It is the internal function that actually does the
/// querying for the public interface methods [`Stash::query_values()`]
/// and [`Tether::query_values()`].
///
/// For full usage details, see [`Stash::query_values()`].
///
/// # Parameters
///
/// * `stash`       - The [`Stash`] instance to use for the query.
/// * `query`       - The query to execute.
/// * `params`      - The parameters to pass to the query.
/// * `conn_handle` - The handle of the connection to use for the query.
///
/// # Errors
///
/// See [`Stash::query()`].
///
/// # See also
///
/// * [`Stash::query()`]
/// * [`Tether::query()`]
/// * [`params!`](crate::utils::params)
///
async fn perform_value_query<Q, T>(
    queue: &QueueSender<Operation>,
    query: Q,
    params: Vec<Box<dyn ToSql + Send>>,
    conn_handle: Option<Arc<AtomicU32>>,
) -> Result<Vec<T>, StashError>
where
    Q: Into<String> + Send,
    T: Clone + Debug + FromSql + ToSql + Send + Sync + PartialEq + 'static,
{
    perform_query::<_, ValueRecord<T>>(queue, query, params, conn_handle)
        .await
        .map(|values| values.into_iter().map(|v| v.value).collect())
}

/// Runs a query and returns any rows of data emitted.
///
/// This function prepares a query and executes it on the database, and returns
/// the resulting rows of data as a collection of instances of the specified `T`
/// type, where `T` is any concrete type implementing the [`DbRecord`] trait. It
/// is the internal function that actually does the querying for the public
/// interface methods [`Stash::query()`] and [`Tether::query()`].
///
/// For full usage details, see [`Stash::query()`].
///
/// # Parameters
///
/// * `stash`       - The [`Stash`] instance to use for the query.
/// * `query`       - The query to execute.
/// * `params`      - The parameters to pass to the query.
/// * `conn_handle` - The handle of the connection to use for the query.
///
/// # Errors
///
/// See [`Stash::query()`].
///
/// # See also
///
/// * [`Stash::query()`]
/// * [`Tether::query()`]
/// * [`params!`](crate::utils::params)
///
async fn perform_query<Q, T>(
    queue: &QueueSender<Operation>,
    query: Q,
    params: Vec<Box<dyn ToSql + Send>>,
    conn_handle: Option<Arc<AtomicU32>>,
) -> Result<Vec<T>, StashError>
where
    Q: Into<String> + Send,
    T: DbRecord + Send + 'static,
    DbRecords: FromIterator<Box<T>>,
{
    let (that_end, this_end) = oneshot::channel();
    let operation = Operation::Query(Query::new(
        Some(that_end),
        conn_handle,
        query.into(),
        params,
        // The converter function picks up the nature of the generic T here, which
        // allows Worker.query() to perform the deserialisation and return the
        // desired type.
        Box::new(converter::<T>),
    ));
    queue
        .send_async(operation)
        .await
        .map_err(|err| StashError::QueueError(err.to_string()))?;
    this_end
        .await
        .map_err(|err| StashError::OneShotError(err.to_string()))??
        .into_iter()
        .map(|item| {
            // The type we receive back is described as Any so that it can pass through
            // the channel without introducing unnecessary type constraints, but is in
            // fact already known to be of type T, so we can downcast it safely.
            item.downcast::<T>()
                .map(|boxed| *boxed)
                .map_err(|_err| StashError::DowncastError)
        })
        .collect()
}

/// Value record struct used to generate the `DbRecord` glue code.
#[derive(Debug, DbRecord, Clone, PartialEq)]
struct ValueRecord<V: Clone + Debug + FromSql + ToSql + Send + Sync + PartialEq + 'static> {
    /// Value we wish to read from the query.
    #[DbField]
    value: V,
}
