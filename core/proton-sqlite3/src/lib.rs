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
//!            let _tx = con.transaction().expect("failed to create transaction");
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

mod migration;
pub mod utils;

use notify::{Config, EventKind, RecursiveMode, Watcher};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::error;

// re-export;
pub use rusqlite;
use rusqlite::{Connection, OpenFlags, Transaction};

pub use migration::*;

pub const DEFAULT_OPEN_CONNECTION_LIMIT: usize = 8;

#[derive(Eq, PartialEq, Copy, Clone)]
enum ConnectionAccess {
    Read,
    Write,
}

/// A connection borrowed from a pool. On drop will be returned to the pool it was acquired from.
pub struct SqliteConnection {
    pool: Arc<ConnectionPoolInner>,
    // Unfortunately we can't transfer this resource on drop, so the only option we have is to wrap it with option.
    conn: Option<Connection>,
    conn_access: ConnectionAccess,
}

impl SqliteConnection {
    /// Convenience transaction wrapper. Creates a new transaction an if the supplied closure does not return an error,
    /// the transaction is committed. On Error, the transaction is rolled back.
    pub fn tx<E: From<rusqlite::Error>, T, F: FnMut(&mut Transaction) -> Result<T, E>>(
        &mut self,
        mut closure: F,
    ) -> Result<T, E> {
        // Default behavior is to roll back the transaction on drop.
        let mut tx = self.deref_mut().transaction()?;
        let value = (closure)(&mut tx)?;

        tx.commit()?;

        Ok(value)
    }

    /// Return the data version of the database. The data version changes every time a change
    /// has been made by another connection.
    pub fn data_version(&self) -> rusqlite::Result<u64> {
        self.deref()
            .pragma_query_value(None, "data_version", |r| r.get(0))
    }
}
impl Drop for SqliteConnection {
    fn drop(&mut self) {
        self.pool
            .release(self.conn.take().unwrap(), self.conn_access);
    }
}

impl Deref for SqliteConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().expect("Should always be valid")
    }
}

impl DerefMut for SqliteConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut().expect("Should always be valid")
    }
}

/// Wrapper type for read only connections.
pub struct ReadOnlySqliteConnection(SqliteConnection);

impl ReadOnlySqliteConnection {
    /// Same as [`SqliteConnection::data_version`].
    pub fn data_version(&self) -> rusqlite::Result<u64> {
        self.0.data_version()
    }
}

impl Deref for ReadOnlySqliteConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
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
    writable_connection: Mutex<Vec<Connection>>,
    readable_connection: Mutex<Vec<Connection>>,
    max_open_connections: usize,
    mode: SqliteMode,
    debug: bool,
}

static SQL_LOG_ONCE: std::sync::Once = std::sync::Once::new();

impl SqliteConnectionPool {
    pub fn new(mode: SqliteMode, debug: bool) -> Self {
        if debug {
            SQL_LOG_ONCE.call_once(|| {
                if let Err(e) = unsafe {
                    rusqlite::trace::config_log(Some(|err_code, log| {
                        error!("[{err_code}]: {log}");
                    }))
                } {
                    error!("Failed to register sqlite log callback: {e}")
                }
            });
        }
        Self::with_open_connections_limit(mode, DEFAULT_OPEN_CONNECTION_LIMIT, debug)
    }

    pub fn with_open_connections_limit(mode: SqliteMode, limit: usize, debug: bool) -> Self {
        Self {
            inner: Arc::new(ConnectionPoolInner {
                mode,
                writable_connection: Mutex::new(Vec::with_capacity(limit)),
                readable_connection: Mutex::new(Vec::new()),
                max_open_connections: limit,
                debug,
            }),
        }
    }

    pub fn acquire(&self) -> rusqlite::Result<SqliteConnection> {
        self.inner
            .get_or_create_connection(ConnectionAccess::Write, self.inner.debug)
            .map(|c| SqliteConnection {
                pool: self.inner.clone(),
                conn: Some(c),
                conn_access: ConnectionAccess::Write,
            })
    }

    pub fn acquire_read_only(&self) -> rusqlite::Result<ReadOnlySqliteConnection> {
        self.inner
            .get_or_create_connection(ConnectionAccess::Read, self.inner.debug)
            .map(|c| {
                ReadOnlySqliteConnection(SqliteConnection {
                    pool: self.inner.clone(),
                    conn: Some(c),
                    conn_access: ConnectionAccess::Read,
                })
            })
    }

    /// Create a watcher for the database which will invoke the handler once an update to the db
    /// has been detected.
    pub fn watch<T: SqliteWatcherHandler>(
        &self,
        handler: T,
    ) -> Result<SqliteWatcher, SqliteWatcherError> {
        self.inner.watch(handler)
    }

    /// Close all connections in the pool. If a connection can't be closed, it will be put back into the pool
    /// and the error is returned.
    pub fn close_all(&self) -> rusqlite::Result<()> {
        self.inner.close_all()
    }
}

impl ConnectionPoolInner {
    fn get_or_create_connection(
        &self,
        connection_access: ConnectionAccess,
        debug: bool,
    ) -> rusqlite::Result<Connection> {
        {
            let mut accessor = match connection_access {
                ConnectionAccess::Write => self.writable_connection.lock().expect("lock poisoning"),
                ConnectionAccess::Read => self.readable_connection.lock().expect("lock poisoning"),
            };
            if let Some(c) = accessor.pop() {
                return Ok(c);
            }
        }

        match connection_access {
            ConnectionAccess::Read => self.new_read_only_connection(debug),
            ConnectionAccess::Write => self.new_connection(debug),
        }
    }

    fn release(&self, conn: Connection, connection_access: ConnectionAccess) {
        {
            let mut accessor = match connection_access {
                ConnectionAccess::Write => self.writable_connection.lock().expect("lock poisoning"),
                ConnectionAccess::Read => self.readable_connection.lock().expect("lock poisoning"),
            };
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

    fn new_read_only_connection(&self, debug: bool) -> rusqlite::Result<Connection> {
        let flags = OpenFlags::empty()
            | OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        self.new_connection_impl(flags, debug)
    }

    fn new_connection_impl(&self, flags: OpenFlags, _debug: bool) -> rusqlite::Result<Connection> {
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

        if _debug {
            conn.trace(Some(|l| {
                tracing::trace!("{l}");
            }));
        }

        Ok(conn)
    }

    fn close_all(&self) -> rusqlite::Result<()> {
        {
            let mut accessor = self.writable_connection.lock().expect("lock poisoning");
            while let Some(c) = accessor.pop() {
                if let Err((c, e)) = c.close() {
                    accessor.push(c);
                    return Err(e);
                }
            }
        }
        {
            let mut accessor = self.readable_connection.lock().expect("lock poisoning");
            while let Some(c) = accessor.pop() {
                if let Err((c, e)) = c.close() {
                    accessor.push(c);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    fn get_wal_path(&self) -> Result<PathBuf, SqliteWatcherError> {
        let SqliteMode::File(path) = &self.mode else {
            return Err(SqliteWatcherError::InvalidMode);
        };

        let mut wal_file = path.clone().into_os_string();
        wal_file.push("-wal");
        Ok(wal_file.into())
    }

    fn watch<T: SqliteWatcherHandler>(
        &self,
        mut handler: T,
    ) -> Result<SqliteWatcher, SqliteWatcherError> {
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
        self.writable_connection
            .lock()
            .expect("lock poisoning")
            .len()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SqliteWatcherError {
    #[error("The watcher is only available when using file based storage")]
    InvalidMode,
    #[error("The watcher has been closed")]
    WatcherClosed,
    #[error("Notify Error: {0}")]
    Notify(#[from] notify::Error),
    #[error("Sqlite Error: {0}")]
    SQL(#[from] rusqlite::Error),
}

/// When watching a database, [`Self::on_db_update`] will be called.
pub trait SqliteWatcherHandler: Send + 'static {
    fn on_db_update(&mut self, v: Result<(), SqliteWatcherError>);
}

impl<T: FnMut(Result<(), SqliteWatcherError>) + Send + 'static> SqliteWatcherHandler for T {
    fn on_db_update(&mut self, v: Result<(), SqliteWatcherError>) {
        (self)(v)
    }
}

/// Observe a database from a [`SqliteConnectionPool`] for changes.
pub struct SqliteWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl SqliteWatcher {
    fn new(watcher: notify::RecommendedWatcher) -> Self {
        Self { _watcher: watcher }
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
