//! Proton Sqlite3 provides some utility classes to correctly initialize and manage sqlite3 connections. It ensures
//! that all sqlite3 connections are initialized with WAL2 and Foreign Key support and provides a `SqliteConnectionPool`
//! abstraction to re-use connections in multithreaded scenarios.
//!
//! ```
//! use proton_sqlite3::SqliteConnectionPool;
//!
//! //let pool = SqliteConnectionPool::new("/tmp/sql.db");
//! fn sql_example(pool: SqliteConnectionPool) {
//!     std::thread::scope( |s| {
//!         let pool_cloned = pool.clone();
//!         s.spawn(move || {
//!            let mut con = pool_cloned.acquire().expect("failed to acquire connection");
//!            let _tx = con.transaction(|tx| -> rusqlite::Result<()>{Ok(())}).expect("failed to create transaction");
//!
//!         });
//!         let pool_cloned = pool.clone();
//!         s.spawn(move || {
//!            let con = pool_cloned.acquire().expect("failed to acquire connection");
//!            let _ = con.execute("SELECT * FROM Users", ()).expect_err("This query fails");
//!         });
//!     });
//! }
//! ```

mod macros;
mod migration;
mod query;
mod tracker;
pub mod utils;
#[cfg(feature = "notify")]
mod watcher;

pub use migration::*;
pub use query::*;
use std::cell::Cell;
pub use tracker::*;

use parking_lot::{Mutex, ReentrantMutex, ReentrantMutexGuard};
use rusqlite::{Connection, OpenFlags, Params, Row, Transaction};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;

// re-export;
pub use paste;
pub use rusqlite;

#[cfg(feature = "notify")]
pub use watcher::*;

pub const DEFAULT_OPEN_CONNECTION_LIMIT: usize = 8;

/// A connection borrowed from a pool. On drop will be returned to the pool it was acquired from.
/// This type wraps around
pub struct SqliteConnection {
    pool: Arc<ConnectionPoolInner>,
    // Unfortunately we can't transfer this resource on drop, so the only option we have is to wrap it with option.
    conn: Option<Connection>,
}

impl SqliteConnection {
    /// Convenience transaction wrapper. Creates a new transaction an if the supplied closure does not return an error,
    /// the transaction is committed. On Error, the transaction is rolled back.
    ///
    /// # Errors
    /// Return errors if the transaction failed or an error occurred during the execution of the
    /// closure.
    pub fn tx<E: From<rusqlite::Error>, T, F: FnOnce(&mut SqliteTransaction) -> Result<T, E>>(
        &mut self,
        closure: F,
    ) -> Result<T, E> {
        self.transaction(closure)
    }

    /// Return the data version of the database. The data version changes every time a change
    /// has been made by another connection.
    ///
    /// # Errors
    /// Returns error if we fail to retrieve the version information.
    pub fn data_version(&self) -> rusqlite::Result<u64> {
        self.connection()
            .pragma_query_value(None, "data_version", |r| r.get(0))
    }

    /// Prepare a sql statement. See [`Connection::prepare()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::prepare()`] for more details.
    #[inline]
    pub fn prepare(&self, sql: impl AsRef<str>) -> rusqlite::Result<rusqlite::Statement<'_>> {
        self.connection().prepare(sql.as_ref())
    }

    /// Execute sql query. See [`Connection::execute()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::execute()`] for more details.
    #[inline]
    pub fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> rusqlite::Result<usize> {
        self.connection().execute(sql.as_ref(), params)
    }

    /// Execute sql query which returns a row. See [`Connection::query_row()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::query_row`] for more details.
    #[inline]
    pub fn query_row<T, P, F>(&self, sql: impl AsRef<str>, params: P, f: F) -> rusqlite::Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        self.connection().query_row(sql.as_ref(), params, f)
    }

    /// Create a new transaction. See [`Connection::transaction()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::transaction()`] for more details.
    #[allow(clippy::missing_panics_doc)]
    pub fn transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        E: From<rusqlite::Error>,
        F: FnOnce(&mut SqliteTransaction) -> Result<T, E>,
    {
        let tx_guard = self.pool.transaction_lock.lock();
        let _nested_tx_guard = NestedScopeGuard::new(tx_guard)?;
        let mut tx = self
            .conn
            .as_mut()
            .expect("Should always have a value")
            .transaction()
            .map(|tx| SqliteTransaction { tx })?;
        let r = f(&mut tx)?;

        tx.commit().map_err(|e| {
            error!("Faile to commit transaction: {e}");
            e
        })?;

        Ok(r)
    }

    /// Get the underlying connection type. Use with caution.
    #[inline]
    pub fn rusqlite_connection(&self) -> &Connection {
        self.connection()
    }

    #[inline]
    fn connection(&self) -> &Connection {
        self.conn.as_ref().expect("should always be available")
    }
}

/// Wrapper around [`Transaction`] in order to ensure there is only one writer to the
/// database in order to avoid database locked errors and catch nested transactions.
pub struct SqliteTransaction<'c> {
    tx: Transaction<'c>,
}

impl<'c> SqliteTransaction<'c> {
    /// Prepare a sql statement. See [`Connection::prepare()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::prepare()`] for more details.
    pub fn prepare(&self, sql: impl AsRef<str>) -> rusqlite::Result<rusqlite::Statement<'_>> {
        self.tx.prepare(sql.as_ref())
    }

    /// Execute sql query. See [`Connection::execute()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::execute()`] for more details.
    #[inline]
    pub fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> rusqlite::Result<usize> {
        self.tx.execute(sql.as_ref(), params)
    }

    /// Execute sql query which returns a row. See [`Connection::query_row()`] for more details.
    ///
    /// # Errors
    /// See [`Connection::query_row()`] for more details.
    #[inline]
    pub fn query_row<T, P, F>(&self, sql: impl AsRef<str>, params: P, f: F) -> rusqlite::Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        self.tx.query_row(sql.as_ref(), params, f)
    }

    /// Commit the transaction.
    ///
    /// # Errors
    /// See [`Transaction::commit()`] for more details.
    pub fn commit(self) -> rusqlite::Result<()> {
        self.tx.commit()
    }

    /// Get the underlying transaction type.
    #[must_use]
    pub fn rusqlite_transaction(&self) -> &Transaction<'_> {
        &self.tx
    }
}

impl Drop for SqliteConnection {
    fn drop(&mut self) {
        self.pool.release(self.conn.take().unwrap());
    }
}

/// Sqlite Database Mode
pub enum SqliteMode {
    /// On disk with WAL2 journaling (Recommended).
    File(PathBuf),
    /// In Memory (For testing only)
    InMemory,
}

/// Manages a pool of Sqlite3 connections and maintains up to a user defined connections open for re-use.
#[derive(Clone)]
pub struct SqliteConnectionPool {
    inner: Arc<ConnectionPoolInner>,
}
struct ConnectionPoolInner {
    connections: Mutex<Vec<Connection>>,
    max_open_connections: usize,
    mode: SqliteMode,
    debug: bool,
    transaction_lock: ReentrantMutex<Cell<bool>>,
}

static SQL_LOG_ONCE: std::sync::Once = std::sync::Once::new();

impl SqliteConnectionPool {
    /// Create a new sqlite connection pool.
    ///
    /// # Params
    /// * `mode`: Sqlite operation mode.
    /// * `debug`: Whether ot enable debug and trace logs.
    pub fn new(mode: SqliteMode, debug: bool) -> Self {
        if debug {
            SQL_LOG_ONCE.call_once(|| {
                if let Err(e) = unsafe {
                    rusqlite::trace::config_log(Some(|err_code, log| {
                        error!("[{err_code}]: {log}");
                    }))
                } {
                    error!("Failed to register sqlite log callback: {e}");
                }
            });
        }
        Self::with_open_connections_limit(mode, DEFAULT_OPEN_CONNECTION_LIMIT, debug)
    }

    /// Create a new sqlite connection pool.
    ///
    /// # Params
    /// * `mode`: Sqlite operation mode.
    /// * `limit`: Number of open connections to maintain open in the pool.
    /// * `debug`: Whether ot enable debug and trace logs.
    #[must_use]
    pub fn with_open_connections_limit(mode: SqliteMode, limit: usize, debug: bool) -> Self {
        Self {
            inner: Arc::new(ConnectionPoolInner {
                mode,
                connections: Mutex::new(Vec::with_capacity(limit)),
                max_open_connections: limit,
                debug,
                transaction_lock: ReentrantMutex::new(Cell::new(false)),
            }),
        }
    }

    /// Acquire a new connection from the pool.
    ///
    /// # Errors
    /// Returns error if we can't obtain a connection.
    pub fn acquire(&self) -> rusqlite::Result<SqliteConnection> {
        self.inner
            .get_or_create_connection(self.inner.debug)
            .map(|c| SqliteConnection {
                pool: self.inner.clone(),
                conn: Some(c),
            })
    }

    /// Create a watcher for the database which will invoke the handler once an update to the db
    /// has been detected.
    #[cfg_attr(doc, features = "notify")]
    #[cfg(feature = "notify")]
    pub fn watch<T: SqliteWatcherHandler>(
        &self,
        handler: T,
    ) -> Result<SqliteWatcher, SqliteWatcherError> {
        self.inner.watch(handler)
    }

    /// Close all connections in the pool. If a connection can't be closed, it will be put back into the pool
    /// and the error is returned.
    ///
    /// # Errors
    /// Return error if we can not close a db connection.
    pub fn close_all(&self) -> rusqlite::Result<()> {
        self.inner.close_all()
    }

    /// Acquire a new connection in an async environment and execute a closure on it.
    ///
    /// Note: Due to sync nature of the library the code is run on the executors sync thread
    /// pool.
    ///
    /// # Errors
    /// Returns error if the connection can not be acquired, an error occurred during the execution
    /// or the blocking task failed to join.
    pub async fn with_async<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        T: Send + 'static,
        E: From<rusqlite::Error> + Send + 'static,
        F: FnOnce(&mut SqliteConnection) -> Result<T, E> + Send + 'static,
    {
        let cloned = self.clone();
        proton_async::runtime::spawn_blocking(move || {
            let mut conn = cloned.acquire()?;
            f(&mut conn)
        })
        .await
        .map_err(|e| {
            rusqlite::Error::UserFunctionError(format!("Failed to join task: {e}").into())
        })?
    }

    /// Acquire a new connection in an async environment, start a transaction and execute a
    /// closure on it.
    ///
    /// Note: Due to sync nature of the library the code is run on the executors sync thread
    /// pool.
    ///
    /// # Errors
    /// Returns error if the connection can not be acquired, an error occurred during the execution
    /// the blocking task failed to join, or the transaction failed to commit.
    pub async fn transaction_async<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        T: Send + 'static,
        E: From<rusqlite::Error> + Send + 'static,
        F: FnOnce(&mut SqliteTransaction) -> Result<T, E> + Send + 'static,
    {
        let cloned = self.clone();
        proton_async::runtime::spawn_blocking(move || {
            let mut conn = cloned.acquire()?;
            conn.transaction(f)
        })
        .await
        .map_err(|e| {
            rusqlite::Error::UserFunctionError(format!("Failed to join task: {e}").into())
        })?
    }

    /// Acquire a connection and execute the given closure on it.
    ///
    /// # Errors
    /// Returns Error if the connection can not be acquired or if the closure returns an error.
    pub fn with<F, R, E>(&self, f: F) -> Result<R, E>
    where
        E: From<rusqlite::Error>,
        F: FnOnce(&mut SqliteConnection) -> Result<R, E>,
    {
        let mut conn = self.acquire()?;
        f(&mut conn)
    }

    /// Acquire a connection start a transaction and run the given closure.
    ///
    /// # Errors
    /// Returns Error if the connection can not be acquired, the closure returns an error or
    /// the transaction failed to commit.
    pub fn transaction<F, R, E>(&self, f: F) -> Result<R, E>
    where
        E: From<rusqlite::Error>,
        F: FnOnce(&mut SqliteTransaction) -> Result<R, E>,
    {
        let mut conn = self.acquire()?;
        conn.transaction(f)
    }
}

impl ConnectionPoolInner {
    fn get_or_create_connection(&self, debug: bool) -> rusqlite::Result<Connection> {
        {
            let mut accessor = self.connections.lock();
            if let Some(c) = accessor.pop() {
                return Ok(c);
            }
        }

        self.new_connection(debug)
    }

    fn release(&self, conn: Connection) {
        {
            let mut accessor = self.connections.lock();
            if accessor.len() < self.max_open_connections {
                accessor.push(conn);
                return;
            }
        }

        if let Err((_, e)) = conn.close() {
            error!("Failed to close connection: {e}");
        }
    }

    fn new_connection(&self, debug: bool) -> rusqlite::Result<Connection> {
        self.new_connection_impl(OpenFlags::default(), debug)
    }

    fn new_connection_impl(&self, flags: OpenFlags, debug: bool) -> rusqlite::Result<Connection> {
        #[allow(unused_mut)]
        let mut conn = match &self.mode {
            SqliteMode::File(path) => {
                let conn = Connection::open_with_flags(path, flags)?;
                conn.pragma_update(None, "synchronous", "FULL")?;
                conn.pragma_update(None, "journal_mode", "WAL")?;
                conn.pragma_update(None, "recursive_triggers", "ON")?;
                conn.pragma_update(None, "temp_store", "MEMORY")?;
                conn
            }
            SqliteMode::InMemory => {
                Connection::open_in_memory_with_flags(flags | OpenFlags::SQLITE_OPEN_SHARED_CACHE)?
            }
        };

        conn.pragma_update(None, "foreign_keys", "ON")?;

        if debug {
            conn.trace(Some(|l| {
                tracing::trace!("{l}");
            }));
        }

        Ok(conn)
    }

    fn close_all(&self) -> rusqlite::Result<()> {
        let mut accessor = self.connections.lock();
        while let Some(c) = accessor.pop() {
            if let Err((c, e)) = c.close() {
                accessor.push(c);
                return Err(e);
            }
        }

        Ok(())
    }

    #[cfg(feature = "notify")]
    fn get_wal_path(&self) -> Result<PathBuf, SqliteWatcherError> {
        let SqliteMode::File(path) = &self.mode else {
            return Err(SqliteWatcherError::InvalidMode);
        };

        let mut wal_file = path.clone().into_os_string();
        wal_file.push("-wal");
        Ok(wal_file.into())
    }

    #[cfg(feature = "notify")]
    fn watch<T: SqliteWatcherHandler>(
        &self,
        mut handler: T,
    ) -> Result<SqliteWatcher, SqliteWatcherError> {
        use notify::{Config, EventKind, RecursiveMode, Watcher};
        let wal_path = self.get_wal_path()?;
        let config = Config::default();

        let mut watcher = notify::RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                let converted = match event {
                    Ok(event) => {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => Ok(()),
                            EventKind::Remove(_) => Err(SqliteWatcherError::WatcherClosed),
                            // We don't handle the other events
                            _ => return,
                        }
                    }
                    Err(e) => Err(e.into()),
                };
                handler.on_db_update(converted)
            },
            config,
        )?;

        watcher.watch(wal_path.as_ref(), RecursiveMode::NonRecursive)?;
        Ok(SqliteWatcher::new(watcher))
    }

    #[cfg(test)]
    fn get_open_connection_count(&self) -> usize {
        self.connections.lock().len()
    }
}

/// Helper type which validates that we are performing a transaction inside another transaction.
/// When dropped will reset the flag state.
struct NestedScopeGuard<'a>(ReentrantMutexGuard<'a, Cell<bool>>);

impl<'a> NestedScopeGuard<'a> {
    /// Create a new guard, which performs a check to see if we are in a nested transaction.
    ///
    /// # Errors
    /// Returns error if we detect a nested transaction.
    fn new(tx_guard: ReentrantMutexGuard<'a, Cell<bool>>) -> rusqlite::Result<Self> {
        if tx_guard.get() {
            return Err(rusqlite::Error::UserFunctionError(
                "Nested transactions are not supported".into(),
            ));
        }
        tx_guard.set(true);

        Ok(Self(tx_guard))
    }
}

impl<'a> Drop for NestedScopeGuard<'a> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[cfg(test)]
fn new_test_dir() -> tempdir::TempDir {
    tempdir::TempDir::new("proton-sqlite3").expect("Failed to create tmp dir")
}
#[test]
fn test_connection_pool() {
    let dir = new_test_dir();
    const CONN_LIMIT: usize = 2;
    let pool = SqliteConnectionPool::with_open_connections_limit(
        SqliteMode::File(dir.path().join("sql.db")),
        CONN_LIMIT,
        false,
    );
    assert_eq!(pool.inner.get_open_connection_count(), 0);

    // Acquire 2 connections and then release them to the pool.
    {
        let _c1 = pool.acquire().expect("failed to acquire");
        assert_eq!(pool.inner.get_open_connection_count(), 0);
        let _c2 = pool.acquire().expect("failed to acquire");
        assert_eq!(pool.inner.get_open_connection_count(), 0);
    }
    assert_eq!(pool.inner.get_open_connection_count(), 2);

    // Acquire 3 connection, 2 should come from the pool and only 2
    {
        let _c1 = pool.acquire().expect("failed to acquire");
        assert_eq!(pool.inner.get_open_connection_count(), 1);
        let _c2 = pool.acquire().expect("failed to acquire");
        assert_eq!(pool.inner.get_open_connection_count(), 0);
        let _c3 = pool.acquire().expect("failed to acquire");
        assert_eq!(pool.inner.get_open_connection_count(), 0);
    }
    assert_eq!(pool.inner.get_open_connection_count(), 2);

    pool.close_all().expect("failed to close all connections");
    assert_eq!(pool.inner.get_open_connection_count(), 0);
}
#[test]
fn test_nested_transactions_trigger_error() {
    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, false);
    let mut conn = pool.acquire().expect("failed to acquire");
    conn.tx(|_| -> rusqlite::Result<()> {
        let mut conn2 = pool.acquire().expect("failed to acquire");
        conn2.tx(|_| -> rusqlite::Result<()> { Ok(()) })
    })
    .expect_err("nested transactions should trigger errors");
}
