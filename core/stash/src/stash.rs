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

use r2d2::{Error as PoolError, Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Error as SqliteError, ToSql};
use serde::de::DeserializeOwned;
use serde_rusqlite::{from_rows, Error as DeserializationError};
use std::path::Path;
use thiserror::Error;
use tokio::task::spawn_blocking;
#[cfg(feature = "uniffi")]
use uniffi::Error as UniffiError;

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

    /// There was a problem with statement execution. Note that this refers to
    /// executing a prepared statement, e.g. actually running a query, and not
    /// the process of preparing the statement/query.
    #[error("Statement execution error: {0}")]
    ExecutionError(SqliteError),

    /// There was a problem with statement preparation. Note that this refers to
    /// preparing a statement from a query and parameters, prior to execution.
    #[error("Statement preparation error: {0}")]
    PreparationError(SqliteError),

    /// There was a problem establishing a tether to the [`Stash`], which could
    /// be to do with creating the actual stash, or connecting to the service.
    #[error("Stash tether error: {0}")]
    TetherError(#[from] PoolError),

    /// There was a problem with thread handling and management. This should
    /// never happen in practice.
    #[error("Thread panic: Join handle failed")]
    ThreadPanic,
}

/// Database interaction interface.
///
/// This struct provides a centralised database-handling interface that
/// manages connections and carries out queries.
///
/// [`Stash`] instances are lightweight, and can be freely cloned without any
/// concerns. When cloned, the new instance will share the same connection pool
/// as the original. This is achieved due to the nature of the [`Pool`] type,
/// which is thread-safe and can be shared across threads. For this reason the
/// [`Stash`] struct is not wrapped in an [`Arc`](std::sync::Arc), and does not
/// need any self-reference.
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
/// [`query()`][Stash::query()] method is asynchronous, and the [`Stash`] struct
/// itself is cloneable and shareable across threads. The [`Stash`] struct uses
/// the [`r2d2`] and [`rusqlite`] crates, which are synchronous, so the
/// [`query()`][Stash::query()] method uses the [`spawn_blocking()`] function to
/// run the blocking synchronous code in a separate thread. This allows the
/// Tokio runtime to continue running other tasks while the blocking code is
/// running.
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
/// it. Whereas, [`spawn_blocking()`] sends the *task* to a thread in the
/// blocking category, allowing the other associated tasks to continue.
///
/// The two main ways of notifying the Tokio runtime that a task is blocking are
/// [`block_in_place()`](tokio::task::block_in_place()) and
/// [`spawn_blocking()`]. The difference is that [`block_in_place()`](tokio::task::block_in_place())
/// blocks the current core thread, whereas [`spawn_blocking()`] spawns a new
/// thread *request* to run the blocking code. Both allow the Tokio runtime to
/// continue running other tasks, and allow the executor to continue in general,
/// but [`block_in_place()`](tokio::task::block_in_place()) will hold up any
/// other tasks running on the current thread, and will prevent the thread from
/// being used for anything else until the work completes.
///
/// It is always importance to consider performance, efficiency, and resource
/// availability when designing asynchronous code. Improper use can lead to
/// exhaustion, starvation, and deadlocks. We do not have to worry about thread
/// pool exhaustion, because Tokio will spawn more blocking threads until the
/// upper limit is reached, after which, the tasks are put into a queue. That
/// means we are free to request new threads as new database queries arise,
/// without concern.
///
/// An alternate pattern would be to create one dedicated system thread for
/// handling database queries, and to send all queries to that thread, usually
/// using an MPSC queue or similar. In current usage this would provide no
/// benefit, as it would just move the point of responsibility from the Tokio
/// runtime to the system thread, and would not provide any additional
/// performance or efficiency. It would also add complexity, and require a
/// mechanism such as oneshot to send the results back to the main thread. By
/// leaning on Tokio's built-in mechanisms we can avoid this complexity.
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
#[derive(Clone, Debug)]
pub struct Stash {
    /// A pool of SQLite connections. The pool is shared across all threads. We
    /// don't need to wrap the [`Pool`] in an [`Arc`] because the [`Pool`]
    /// itself is already thread-safe, being `Pool<M>(Arc<SharedPool<M>>)`
    /// underneath.
    pool: Pool<SqliteConnectionManager>,
}

impl Stash {
    /// Creates a new [`Stash`] instance.
    ///
    /// This function creates a new [`Stash`] instance with a new SQLite
    /// connection pool. The pool is created with the given file path.
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
        let manager = path.map_or_else(
            SqliteConnectionManager::memory,
            SqliteConnectionManager::file,
        );
        let pool = Pool::new(manager).map_err(StashError::TetherError)?;
        Ok(Self { pool })
    }

    /// Gets a connection from the pool.
    ///
    /// This function gets a connection from the pool. The connection is
    /// returned as a [`PooledConnection`], which is a smart pointer that
    /// automatically returns the connection to the pool when it goes out of
    /// scope.
    ///
    /// In practice it should not be necessary to call this method in normal
    /// day-to-day operation, as the [`Stash`] struct provides all the necessary
    /// functionality. It is provided for completeness and for any cases where
    /// direct access to a database connection is required.
    ///
    /// # Errors
    ///
    /// Note that this function returns a [`PoolError`], which is a type alias
    /// for the error type returned by the [`r2d2`] crate. This is not converted
    /// to any [`StashError`] variant, and is left as-is, as this function is a
    /// low-level one that provides a direct connection.
    ///
    pub fn connection(&self) -> Result<PooledConnection<SqliteConnectionManager>, PoolError> {
        self.pool.get()
    }

    /// Prepares and executes a query, and returns the number of affected rows.
    ///
    /// This function executes a query on the database and indicates whether it
    /// was successful, returning the number of affected rows. It does not
    /// return any rows of data that the query may have emitted, and is designed
    /// for situations where no data is expected, such as `INSERT`, `UPDATE`, or
    /// `DELETE` queries.
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
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///
    /// Note that, unlike the [`query()`][Stash::query()] method, no distinction
    /// is made between execution and preparation errors.
    ///
    pub async fn execute<Q: Into<String> + Send>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<(), StashError> {
        let pool = self.pool.clone();
        #[allow(clippy::shadow_reuse)]
        let query = query.into();

        // Spawn a blocking task to execute the query. This is necessary because
        // rusqlite is synchronous, so we need to tell the Tokio runtime that
        // this task will block.
        spawn_blocking(move || {
            let connection = pool.get().map_err(StashError::TetherError)?;
            let params_refs: Vec<&dyn ToSql> = params
                .iter()
                .map(|p| {
                    #[allow(clippy::shadow_same)]
                    let p: &dyn ToSql = &**p;
                    p
                })
                .collect();
            let _: usize = connection
                .execute(&query, &*params_refs)
                .map_err(StashError::ExecutionError)?;
            Ok(())
        })
        .await
        .map_err(|_err| StashError::ThreadPanic)?
    }

    /// Gets the connection pool.
    ///
    /// This function returns a clone of the connection pool. The pool is
    /// thread-safe and shareable.
    ///
    /// In practice it should not be necessary to call this method in normal
    /// day-to-day operation, as the [`Stash`] struct provides all the necessary
    /// functionality. It is provided for completeness and for any cases where
    /// direct access to the pool is required.
    ///
    #[must_use]
    pub fn pool(&self) -> Pool<SqliteConnectionManager> {
        self.pool.clone()
    }

    /// Prepares and executes a query, and returns any rows of data emitted.
    ///
    /// This function executes a query on the database and returns the result as
    /// a collection of instances of the specified `T` type, where `T` is any
    /// concrete type implementing the [`DeserializeOwned`] trait. The
    /// requirement to formalise the return type streamlines the process of
    /// handling the results.
    ///
    /// Although this function is *designed* for read queries, this is implied
    /// and a convenience, and it is entirely possible to use it for write
    /// queries as well. The number of rows returned will be zero for write
    /// queries. Any semantic difference between read and write queries is left
    /// to the caller to decide, and does not result in any difference in
    /// handling by this module. The [`rusqlite`] library will handle the
    /// distinction as necessary.
    ///
    /// Note that it is possible to deserialise the results into other types
    /// too, and indeed serialise types into queries, but those use cases are
    /// unlikely to be needed, or at least not common, and so are not provided
    /// by this module. They can be achieved, if necessary, by running queries
    /// manually after obtaining a connection using the [`connection()`][Stash::connection()]
    /// method.
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
    ///   - [`PreparationError`](StashError::PreparationError) - Problem
    ///     preparing the query.
    ///   - [`TetherError`](StashError::TetherError) - Problem obtaining a
    ///     connection from the pool.
    ///
    pub async fn query<Q, T>(
        &self,
        query: Q,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<T>, StashError>
    where
        Q: Into<String> + Send,
        T: DeserializeOwned + Send + 'static,
    {
        let pool = self.pool.clone();
        #[allow(clippy::shadow_reuse)]
        let query = query.into();

        // Spawn a blocking task to execute the query. This is necessary because
        // rusqlite is synchronous, so we need to tell the Tokio runtime that
        // this task will block.
        spawn_blocking(move || {
            let connection = pool.get().map_err(StashError::TetherError)?;
            let params_refs: Vec<&dyn ToSql> = params
                .iter()
                .map(|p| {
                    #[allow(clippy::shadow_same)]
                    let p: &dyn ToSql = &**p;
                    p
                })
                .collect();
            let mut statement = connection
                .prepare(&query)
                .map_err(StashError::PreparationError)?;
            let rows: Result<Vec<T>, DeserializationError> = from_rows(
                statement
                    .query(&*params_refs)
                    .map_err(StashError::ExecutionError)?,
            )
            .collect();
            rows.map_err(StashError::DeserializationError)
        })
        .await
        .map_err(|_err| StashError::ThreadPanic)?
    }
}
