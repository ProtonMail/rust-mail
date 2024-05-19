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
//! use in place of the "real" connections, as they are not thread-safe. The
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
//! through a single worker thread is an obvious bottleneck. This can be
//! improved or resolved by adding additional workers to process the queue, but
//! that may or may not be desirable.
//!
//! The approach to logic using this module also needs to be thought through
//! carefully in any situation where transactions are used. As a rule of thumb,
//! code using transactions should be as close to hand as possible (i.e. to
//! minimise unseen effects), and should keep the transaction open for as short
//! a time as possible.
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
//!      cause any reduction in performance, considering it prevents parallel
//!      read operations?**
//!
//!      The current design is expected to be more than adequate for the target
//!      usage. As a general statement, it is indeed quicker to carry out read
//!      operations in parallel, but this can easily be achieved in future, if
//!      required, by increasing the number of workers and worker threads
//!      processing the queue.
//!

use core::any::Any;
use core::ops::Deref;
use flume::Sender as QueueSender;
use r2d2::{Error as PoolError, Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Error as SqliteError, Rows, ToSql};
use serde::de::DeserializeOwned;
use serde_rusqlite::{from_rows, Error as DeserializationError};
use std::collections::{hash_map::Entry, HashMap};
use std::path::Path;
use std::sync::{Arc, Weak};
use std::thread::spawn;
use thiserror::Error;
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tracing::error;
#[cfg(feature = "uniffi")]
use uniffi::Error as UniffiError;

/// Shorthand for the converted query results, i.e. the [`Rows`] that have been
/// converted into the desired type `T`, but boxed as [`Any`] so that they can
/// be returned via the oneshot channel.
///
/// # See also
///
/// * [`Query::run()`]
/// * [`Stash::query()`]
/// * [`Tether::query()`]
/// * [`converter()`]
///
type AnyRecords = Vec<Box<dyn Any + Send>>;

/// A dual-state connection wrapper.
///
/// This enum works in similar fashion to [`Cow`](std::borrow::Cow), allowing
/// the connection to be either borrowed or owned.
///
/// It implements [`Deref`] so that it is essentially invisible to the caller.
///
enum AgnosticConnection<'a> {
    /// A borrowed connection.
    Borrowed(&'a PooledConnection<SqliteConnectionManager>),

    /// An owned connection.
    Owned(PooledConnection<SqliteConnectionManager>),
}

impl Deref for AgnosticConnection<'_> {
    type Target = PooledConnection<SqliteConnectionManager>;

    fn deref(&self) -> &Self::Target {
        #[allow(clippy::match_same_arms)]
        match *self {
            Self::Borrowed(s) => s,
            // This only actually needs to return s and not &s, as &s would immediately
            // get dereferenced to s. So, counter-intuitively, these arms do the same
            // thing - but the key is in the context of usage.
            Self::Owned(ref s) => s,
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
/// * [`Instruction`]
/// * [`OperationLogic`]
/// * [`Query`]
/// * [`Worker`]
///
enum Operation {
    /// A query to be executed, where no results are expected. This is usually
    /// a write query, or a command, but differentiation is up to the caller and
    /// not enforced.
    Instruct(Instruction),

    /// A query to be executed, where results are expected. This is typically a
    /// read query, but could be any query where results are expected, such as
    /// an `INSERT` query that returns the ID of the inserted row.
    Query(Query),
}

/// Error type for the [`Stash`] module.
#[derive(Debug, Error)]
#[cfg_attr(feature = "uniffi", derive(UniffiError))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
#[non_exhaustive]
pub enum StashError {
    /// There was a problem with deserialising the query results. This means
    /// that serde failed to convert the query results into the desired type,
    /// which could be due to a mismatch between the query results and the
    /// expected type.
    #[error("Query results deserialisation error: {0}")]
    DeserializationError(#[from] DeserializationError),

    /// A problem was experienced when attempting to downcast a boxed trait
    /// object. This should never happen in practice.
    #[error("Downcast error")]
    DowncastError,

    /// There was a problem with statement execution. Note that this refers to
    /// executing a prepared statement, e.g. actually running a query, and not
    /// the process of preparing the statement/query.
    #[error("Statement execution error: {0}")]
    ExecutionError(SqliteError),

    /// There was a problem with statement preparation. Note that this refers to
    /// preparing a statement from a query and parameters, prior to execution.
    #[error("Statement preparation error: {0}")]
    PreparationError(SqliteError),

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

    /// There was a problem establishing a tether to the [`Stash`], which could
    /// be to do with creating the actual stash, or connecting to the service.
    #[error("Stash tether error: {0}")]
    TetherError(#[from] PoolError),

    /// There was a problem with thread handling and management. This should
    /// never happen in practice.
    #[error("Thread panic: Join handle failed")]
    ThreadPanic,
}

/// An operation to be executed by the worker, which does not return any data.
///
/// This is used for operations such as `INSERT`, `UPDATE`, and `DELETE`, where
/// the result is the number of rows affected, along with other similar
/// commands.
///
/// # See also
///
/// * [`Operation`]
/// * [`Query`]
///
struct Instruction {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: OneshotSender<Result<usize, StashError>>,

    /// The unique handle of the connection to use for the query. If [`Some`] a
    /// database connection will be created and associated if not already
    /// registered, and re-used otherwise. If [`None`], a new database
    /// connection will be created, but not registered, and used just this once.
    conn_handle: Option<Arc<()>>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl OperationLogic for Instruction {
    type Output = usize;

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
        connection
            .execute(&self.query, &*Self::prepare_params(&self.params))
            .map_err(StashError::ExecutionError)
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
/// * [`Instruction`]
/// * [`Operation`]
///
struct Query {
    /// The communication channel used to send the result of the operation back
    /// to the caller.
    channel: OneshotSender<Result<AnyRecords, StashError>>,

    /// The unique handle of the connection to use for the query. If [`Some`] a
    /// database connection will be created and associated if not already
    /// registered, and re-used otherwise. If [`None`], a new database
    /// connection will be created, but not registered, and used just this once.
    conn_handle: Option<Arc<()>>,

    /// The deserialisation function to use to convert the query results into
    /// the desired type. This is necessary because the [`Rows`] type returned
    /// by the [`rusqlite`] library is not thread-safe.
    #[allow(clippy::type_complexity)]
    converter: Box<dyn Fn(Rows<'_>) -> Result<AnyRecords, DeserializationError> + Send>,

    /// The parameters to pass to the query. These are boxed trait objects that
    /// implement the [`ToSql`] trait, and are `Send` so that they can be sent
    /// between threads.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query to execute. This is in raw SQL format ready for parameter
    /// substitution.
    query: String,
}

impl OperationLogic for Query {
    type Output = AnyRecords;

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
    fn run(&self, connection: &AgnosticConnection<'_>) -> Result<AnyRecords, StashError> {
        let mut statement = connection
            .prepare(&self.query)
            .map_err(StashError::PreparationError)?;
        let rows: Result<AnyRecords, DeserializationError> = (self.converter)(
            statement
                .query(&*Self::prepare_params(&self.params))
                .map_err(StashError::ExecutionError)?,
        );
        rows.map_err(StashError::DeserializationError)
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
/// # Approach to async
///
/// The [`Stash`] struct is designed to be used in an asynchronous context. The
/// [`query()`][Stash::query()] and [`execute()`][Stash::execute()] methods are
/// asynchronous (as are their connection-specific [`Tether`] counterparts), and
/// the [`Stash`] struct itself is cloneable and shareable across threads. The
/// database handling uses the [`r2d2`] and [`rusqlite`] crates, which are
/// synchronous, so they are handled in a separate background thread by a worker
/// to avoid blocking the main Tokio runtime, and to ensure that there is a
/// synchronous "funnel" to handle all database operations.
///
/// As the various [`rusqlite`] types are not [`Send`] compatible, they cannot
/// be passed between threads, and so cannot cross the async boundary. Therefore
/// this approach of the background worker and the [`Tether`] struct is
/// necessary to provide a thread-safe and async-compatible interface to the
/// database.
///
/// It is important to use the [`spawn_blocking()`](tokio::task::spawn_blocking)
/// function to run any blocking synchronous code in a separate thread. This
/// allows the Tokio runtime to continue running other tasks while the blocking
/// code is running.
///
/// This is important because otherwise the executor would be blocked, and Tokio
/// would not be able to run other tasks. To clarify: the mechanism by which the
/// Tokio runtime operates is that of work scheduling. It will run the various
/// work units (tasks) that it has against the available OS threads, via
/// allocated "core" threads, and will switch between them (i.e. between the
/// tasks) as necessary, allocating the tasks against the core threads according
/// to its work management priorities. If a task blocks, then the thread that it
/// is running on will be blocked, and the Tokio runtime will not be able to run
/// other tasks on that thread.
///
/// Bear in mind that asking Tokio to create a new "thread" is not the same as
/// creating a new OS thread. Tokio uses a thread pool, and manages the work
/// units, each of which *can* operate on a separate thread, allocating the work
/// units to the available core OS threads as needed. For this reason, it is
/// important to notify the Tokio runtime when a task is going to issue a
/// blocking call (e.g. waiting on file or network I/O), or perform a lot of
/// compute without yielding. Such a situation can prevent the executor from
/// driving other tasks forward, and can lead to a deadlock. Notifying the
/// executor allows it to hand off any other tasks it has to a new core thread
/// before the blocking call is made. Tokio handles blocking situations
/// separately, in blocking threads, which are separate from the core threads.
///
/// Tokio has two kinds of threads in its thread pool: core (OS) threads, and
/// blocking threads. By default, Tokio will create one core thread for each CPU
/// core, and up to around 500 blocking threads. Using [`block_in_place()`](tokio::task::block_in_place())
/// temporarily *changes* the current thread category from core to blocking,
/// allowing the runtime to spawn another core thread to handle things while the
/// blocking code runs. Because the whole thread categorisation is changed,
/// anything else (i.e. other tasks) associated with the thread are taken with
/// it. Whereas, [`spawn_blocking()`](tokio::task::spawn_blocking) sends the
/// *task* to a thread in the blocking category, allowing the other associated
/// tasks to continue.
///
/// The two main ways of notifying the Tokio runtime that a task is blocking are
/// [`block_in_place()`](tokio::task::block_in_place()) and [`spawn_blocking()`](tokio::task::spawn_blocking).
/// The difference is that [`block_in_place()`](tokio::task::block_in_place())
/// blocks the current core thread, whereas [`spawn_blocking()`](tokio::task::spawn_blocking)
/// spawns a new thread *request* to run the blocking code. Both allow the Tokio
/// runtime to continue running other tasks, and allow the executor to continue
/// in general, but [`block_in_place()`](tokio::task::block_in_place()) will
/// hold up any other tasks running on the current thread, and will prevent the
/// thread from being used for anything else until the work completes.
///
/// It is always importance to consider performance, efficiency, and resource
/// availability when designing asynchronous code. Improper use can lead to
/// exhaustion, starvation, and deadlocks. We do not have to worry about thread
/// pool exhaustion, because Tokio will spawn more blocking threads until the
/// upper limit is reached, after which, the tasks are put into a queue. That
/// means we are free to request new threads as new database queries arise,
/// without concern.
///
/// As a rule of thumb, async code should never run for too long between `await`
/// occurrences. This is because the Tokio runtime uses cooperative scheduling,
/// and will not interrupt a task that is running. Hence care should be taken to
/// identify those places that may block, especially when using synchronous
/// libraries. On the other hand, over-use of async can cause performance
/// degradation due to the overhead of task management, mainly the time taken to
/// switch tasks between threads. Notably, it is in this area that Go tends to
/// outperform Rust, because Go uses a different threading model with
/// goroutines. The Tokio approach of essentially hibernating and reviving tasks
/// is more complex, but allows for more fine-grained control and better
/// resource management, and increased predictability and confidence. Therefore,
/// it is important to only make async those functions that need to be async
/// (bearing in mind the "polluting" effect of async on the codebase), and not
/// to just make everything async by default. In reality, providing these basic
/// guidelines are followed, operational issues are rare, and performance is
/// generally very good.
///
/// However, due to the async-safe implementation, there is currently no need to
/// use the [`spawn_blocking()`](tokio::task::spawn_blocking) function to run
/// blocking synchronous code in a separate thread. The background worker
/// operates synchronously, in its own thread, and all of the interactions with
/// it via queues and channels are async-safe. Therefore these notes are
/// provided for general information and context, and to guide future
/// development.
///
#[derive(Clone, Debug)]
pub struct Stash {
    /// The sender for the stash operations. This is used to send operations to
    /// the worker thread for execution. This is the manner by which the order
    /// of operations is maintained, and how connections are managed and made
    /// thread-safe.
    queue: QueueSender<Operation>,
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
        Ok(Self {
            queue: Worker::start(path)?,
        })
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
    #[must_use]
    pub fn connection(&self) -> Tether {
        Tether {
            handle: Arc::new(()),
            queue: self.queue.clone(),
        }
    }

    /// Runs a query against a new connection, and returns the affected row
    /// count.
    ///
    /// This function prepares a query and executes it on the database, and then
    /// indicates whether it was successful, returning the number of affected
    /// rows. It does not return any rows of data that the query may have
    /// emitted, and is designed for situations where no data is expected, such
    /// as `INSERT`, `UPDATE`, or `DELETE` queries.
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
    /// The query will be executed against a new database connection created
    /// specifically for its use. For once-off, unrelated queries this is fine,
    /// but when using transactions it is critical to run all related queries
    /// against the same connection, in which case use [`Tether::execute()`].
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
    /// * [`Stash::query()`]
    /// * [`Tether::execute()`]
    ///
    pub async fn execute<Q: Into<String> + Send>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<usize, StashError> {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::Instruct(Instruction {
            channel: that_end,
            conn_handle: None,
            query: query.into(),
            params,
        });
        self.queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))?
    }

    /// Runs a query against a new connection, and returns any rows of data
    /// emitted.
    ///
    /// This function prepares a query and executes it on the database, and
    /// returns the resulting rows of data as a collection of instances of the
    /// specified `T` type, where `T` is any concrete type implementing the
    /// [`DeserializeOwned`] trait. The requirement to formalise the return type
    /// streamlines the process of handling the results.
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
    /// The query will be executed against a new database connection created
    /// specifically for its use. For once-off, unrelated queries this is fine,
    /// but when using transactions it is critical to run all related queries
    /// against the same connection, in which case use [`Tether::query()`].
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
    ///     from [`Any`] to `T`.
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
    /// * [`Stash::execute()`]
    /// * [`Tether::query()`]
    ///
    pub async fn query<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String> + Send,
        T: DeserializeOwned + Send + 'static,
        Vec<Box<(dyn Any + Send + 'static)>>: FromIterator<Box<T>>,
    {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::Query(Query {
            channel: that_end,
            conn_handle: None,
            query: query.into(),
            params,
            // The converter function picks up the nature of the generic T here, which
            // allows Worker.query() to perform the deserialisation and return the
            // desired type.
            converter: Box::new(converter::<T>),
        });
        self.queue
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
}

/// Database connection context.
///
/// This struct provides a lightweight, thread-safe database connection context
/// — which is not an actual connection, but a tether to one — that can be
/// shared easily and without concern. It is used to execute queries against the
/// database, but more importantly provides an associative context for handling
/// transactions, as all queries within a transaction must be executed against
/// the same connection.
///
/// # Cloning
///
/// [`Tether`] instances are lightweight, and can be freely cloned without any
/// concerns, as all of their internals are wrapped in [`Arc`]s. For this reason
/// the [`Tether`] struct is not itself wrapped in an [`Arc`], and does not need
/// any self-reference.
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
#[derive(Clone, Debug)]
pub struct Tether {
    /// A reference-counted pointer to an immutable internal handle, which is
    /// used to identify and specify the associated connection when any database
    /// operations are carried out. The handle is simply a unit, as the value
    /// does not matter, only the unique instance.
    handle: Arc<()>,

    /// The queue for the [`Worker`] and [`Stash`] to which the [`Tether`] is
    /// associated. This is used to send queries to the worker for execution.
    queue: QueueSender<Operation>,
}

impl Tether {
    /// Runs a query against an open connection, and returns the affected row
    /// count.
    ///
    /// This function prepares a query and executes it on the database, and then
    /// indicates whether it was successful, returning the number of affected
    /// rows.
    ///
    /// **Note: This function is connection-specific, i.e. the query will be
    /// executed in context to the [`Tether`] instance, against the associated
    /// database connection. For full documentation on this function's
    /// behaviour, see the main documentation for [`Stash::execute()`], which is
    /// standalone and executes each query on a new connection.**
    ///
    /// # Parameters
    ///
    /// * `query`  - The query to execute.
    /// * `params` - The parameters to pass to the query.
    ///
    /// # Errors
    ///
    /// See [`Stash::execute()`].
    ///
    /// # See also
    ///
    /// * [`Stash::execute()`]
    /// * [`Tether::query()`]
    ///
    pub async fn execute<Q: Into<String> + Send>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<usize, StashError> {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::Instruct(Instruction {
            channel: that_end,
            conn_handle: Some(Arc::clone(&self.handle)),
            query: query.into(),
            params,
        });
        self.queue
            .send_async(operation)
            .await
            .map_err(|err| StashError::QueueError(err.to_string()))?;
        this_end
            .await
            .map_err(|err| StashError::OneShotError(err.to_string()))?
    }

    /// Runs a query against an open connection, and returns any rows of data
    /// emitted.
    ///
    /// This function prepares a query and executes it on the database, and
    /// returns the resulting rows of data as a collection of instances of the
    /// specified `T` type, where `T` is any concrete type implementing the
    /// [`DeserializeOwned`] trait.
    ///
    /// **Note: This function is connection-specific, i.e. the query will be
    /// executed in context to the [`Tether`] instance, against the associated
    /// database connection. For full documentation on this function's
    /// behaviour, see the main documentation for [`Stash::query()`], which is
    /// standalone and executes each query on a new connection.**
    ///
    /// # Parameters
    ///
    /// * `query`  - The query to execute.
    /// * `params` - The parameters to pass to the query.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`].
    ///
    /// # See also
    ///
    /// * [`Stash::query()`]
    /// * [`Tether::execute()`]
    ///
    pub async fn query<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String> + Send,
        T: DeserializeOwned + Send + 'static,
        Vec<Box<(dyn Any + Send + 'static)>>: FromIterator<Box<T>>,
    {
        let (that_end, this_end) = oneshot::channel();
        let operation = Operation::Query(Query {
            channel: that_end,
            conn_handle: Some(Arc::clone(&self.handle)),
            query: query.into(),
            params,
            // The converter function picks up the nature of the generic T here, which
            // allows Worker.query() to perform the deserialisation and return the
            // desired type.
            converter: Box::new(converter::<T>),
        });
        self.queue
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
    /// A pool of SQLite connections. Although the pool itself is thread-safe,
    /// being `Pool<M>(Arc<SharedPool<M>>)` underneath, the connections are not.
    /// Therefore we store the pool centrally on the worker, keep the created
    /// connections on the worker, and issue thread-safe [`Tether`]s to the
    /// caller.
    pool: Pool<SqliteConnectionManager>,

    /// A map of active connections. This is used to keep track of the
    /// connections that are currently in use, and to associate them with the
    /// [`Tether`]s that are issued to the caller. The connections are stored as
    /// [`PooledConnection`]s, which are not thread-safe, and so are not
    /// directly accessible by the caller.
    ///
    /// A weak reference to the connection handle is also stored, so that the
    /// connection can be re-used if it is already registered, but also removed
    /// from the list if it is no longer in use.
    ///
    /// Note that the key is the *pointer* to the weak reference, and not the
    /// actual weak reference itself. This is because a `Weak<()>` cannot be a
    /// [`HashMap`] key. Use of a pointer here is safe, as the pointer is unique
    /// to the connection, and is only used for the purpose of identification.
    tethers: HashMap<*const (), (Weak<()>, PooledConnection<SqliteConnectionManager>)>,
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
    /// * `path` - The path to the SQLite database file. If `None`, an in-memory
    ///            database is created.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem creating
    /// the database or connection pool.
    ///
    fn start(path: Option<&Path>) -> Result<QueueSender<Operation>, StashError> {
        let manager = path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );
        let (sender, receiver) = flume::unbounded();
        let pool = Pool::new(manager).map_err(StashError::TetherError)?;

        // Spawn a thread to run the worker. This thread will execute the queries
        // sequentially, as they are received, and will return the results via
        // oneshot channels.
        drop(spawn(move || {
            let mut worker = Self {
                pool,
                tethers: HashMap::new(),
            };

            while let Ok(operation) = receiver.recv() {
                let conn_handle = match operation {
                    Operation::Instruct(ref instruction) => instruction.conn_handle.clone(),
                    Operation::Query(ref query) => query.conn_handle.clone(),
                };
                let connection_result = match conn_handle {
                    Some(ref handle) => worker
                        .get_connection(handle)
                        .map(AgnosticConnection::Borrowed),
                    None => worker
                        .pool
                        .get()
                        .map(AgnosticConnection::Owned)
                        .map_err(StashError::TetherError),
                };
                let connection = match connection_result {
                    Ok(connection) => connection,
                    Err(error) => {
                        if match operation {
                            Operation::Instruct(instruction) => {
                                instruction.channel.send(Err(error)).map_err(|_err| ())
                            }
                            Operation::Query(query) => {
                                query.channel.send(Err(error)).map_err(|_err| ())
                            }
                        }
                        .is_err()
                        {
                            error!("Oneshot error: Failed sending error back to caller");
                        }
                        continue;
                    }
                };
                if (match operation {
                    Operation::Instruct(instruction) => {
                        let result = instruction.run(&connection);
                        instruction.channel.send(result).map_err(|_err| ())
                    }
                    Operation::Query(query) => {
                        let result = query.run(&connection);
                        query.channel.send(result).map_err(|_err| ())
                    }
                })
                .is_err()
                {
                    // If sending down the oneshot channel fails, send() returns the message to
                    // us. It's not particularly interesting what that message is, as we never
                    // expect this to fail, so we erase the error details and just log the error
                    // event. If we do later want to capture the message in the logs, error!()
                    // will have to be apply for each case above, as the message types differ.
                    error!("Oneshot error: Failed sending result back to caller");
                }
            }
        }));

        Ok(sender)
    }

    /// Gets a connection from the pool.
    ///
    /// This function gets a connection from the pool, or creates one and
    /// registers it for re-use.
    ///
    /// The internal list of associated [`Tether`] connection handles is checked
    /// to see if the connection is already registered. If it is, the existing
    /// connection is returned. If it is not, a new connection is created and
    /// registered, and returned. A registration is made by storing a weak
    /// reference to the connection handle supplied from the [`Tether`]
    /// instance, against the actual [`PooledConnection`].
    ///
    /// If the specified connection handle is not already registered then it
    /// means that this is a new connection request, as the process of
    /// requesting a new connection is disassociated from the actual acquisition
    /// of the connection itself. This is because the connection is only created
    /// when the first query is executed, and so the [`Tether`] is created and
    /// returned immediately, with no delay.
    ///
    /// The connection will be returned to the pool by garbage collection once
    /// the [`Tether`] goes out of scope, as the strong reference will expire.
    ///
    /// # Parameters
    ///
    /// * `conn_handle` - The handle of the connection to use for the query. A
    ///                   database connection will be created and associated if
    ///                   not already registered, and re-used otherwise.
    ///
    /// # Errors
    ///
    /// A [`StashError::TetherError`] is returned if there is a problem
    /// obtaining a connection from the pool.
    ///
    /// # See also
    ///
    /// * [`Stash::connection()`]
    /// * [`Tether`]
    ///
    fn get_connection(
        &mut self,
        conn_handle: &Arc<()>,
    ) -> Result<&PooledConnection<SqliteConnectionManager>, StashError> {
        let weak_ref = Arc::downgrade(conn_handle);
        // This code uses the Entry API to avoid double mutable borrow of self.
        match self.tethers.entry(weak_ref.as_ptr()) {
            Entry::Occupied(entry) => Ok(&entry.into_mut().1),
            Entry::Vacant(entry) => {
                let connection = self.pool.get().map_err(StashError::TetherError)?;
                Ok(&entry.insert((weak_ref, connection)).1)
            }
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
/// * [`Instruction`]
/// * [`Operation`]
/// * [`Query`]
///
trait OperationLogic {
    /// The type of the output of the operation, i.e. what is returned by the
    /// [`run()`](OperationLogic::run()) method's implementation.
    type Output;

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
/// [`DeserializeOwned`] trait. We don't want to restrict the caller to a
/// specific type, or even an enumerated list of types, and neither to we want
/// to serialise the results into intermediary form to unpack at the other end
/// of the queue. We therefore use `Box<dyn Any>` for a very short and specific
/// purpose, which is to send the results back to the caller via the oneshot
/// channel. They have in fact already been converted at this point, but must be
/// passed generically and then downcast. This method of transport is therefore
/// the most efficient option we can choose, and bears a very slight overhead of
/// type manipulation, but does not introduce any wider dynamic dispatch or
/// unnecessary byte manipulation (as the deserialisation happens exactly once).
///
/// # Parameters
///
/// * `rows` - The rows of data returned by the query. These will be converted
///            to the type specified by `T`.
///
/// # Errors
///
/// A [`DeserializationError`] is returned if there is a problem deserialising
/// the query results. This will then be converted into a
/// [`StashError::DeserializationError`] by the caller.
///
fn converter<T>(rows: Rows<'_>) -> Result<AnyRecords, DeserializationError>
where
    T: DeserializeOwned + Send + 'static,
    AnyRecords: FromIterator<Box<T>>,
{
    from_rows::<T>(rows).map(|res| res.map(Box::new)).collect()
}
