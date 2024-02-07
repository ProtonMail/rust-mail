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

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::error;

// re-export;
pub use rusqlite;
use rusqlite::{Connection, OpenFlags, Transaction};

pub use migration::*;

pub const DEFAULT_OPEN_CONNECTION_LIMIT: usize = 8;

/// A connection borrowed from a pool. On drop will be returned to the pool it was acquired from.
pub struct SqliteConnection {
    pool: Arc<ConnectionPoolInner>,
    // Unfortunately we can't transfer this resource on drop, so the only option we have is to wrap it with option.
    conn: Option<Connection>,
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
}
impl Drop for SqliteConnection {
    fn drop(&mut self) {
        self.pool.release(self.conn.take().unwrap());
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
    lock: Mutex<Vec<Connection>>,
    max_open_connections: usize,
    mode: SqliteMode,
}

impl SqliteConnectionPool {
    pub fn new(mode: SqliteMode) -> Self {
        Self::with_open_connections_limit(mode, DEFAULT_OPEN_CONNECTION_LIMIT)
    }

    pub fn with_open_connections_limit(mode: SqliteMode, limit: usize) -> Self {
        Self {
            inner: Arc::new(ConnectionPoolInner {
                mode,
                lock: Mutex::new(Vec::with_capacity(limit)),
                max_open_connections: limit,
            }),
        }
    }

    pub fn acquire(&self) -> rusqlite::Result<SqliteConnection> {
        self.inner
            .get_or_create_connection()
            .map(|c| SqliteConnection {
                pool: self.inner.clone(),
                conn: Some(c),
            })
    }

    /// Close all connections in the pool. If a connection can't be closed, it will be put back into the pool
    /// and the error is returned.
    pub fn close_all(&self) -> rusqlite::Result<()> {
        self.inner.close_all()
    }
}

impl ConnectionPoolInner {
    fn get_or_create_connection(&self) -> rusqlite::Result<Connection> {
        {
            let mut accessor = self.lock.lock().expect("lock poisoning");
            if let Some(c) = accessor.pop() {
                return Ok(c);
            }
        }

        self.new_connection()
    }

    fn release(&self, conn: Connection) {
        {
            let mut accessor = self.lock.lock().expect("lock poisoning");
            if accessor.len() < self.max_open_connections {
                accessor.push(conn);
                return;
            }
        }

        if let Err((_, e)) = conn.close() {
            error!("Failed to close connection: {e}");
        }
    }

    fn new_connection(&self) -> rusqlite::Result<Connection> {
        let conn = match &self.mode {
            SqliteMode::File(path) => {
                let conn = Connection::open(path)?;
                conn.execute("PRAGMA synchronous=NORMAL;", ())?;
                conn.execute("PRAGMA journal=WAL2;", ())?;
                conn
            }
            SqliteMode::InMemory => Connection::open_in_memory_with_flags(
                OpenFlags::default() | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
            )?,
        };

        conn.execute("PRAGMA foreign_keys = ON;", ())?;

        Ok(conn)
    }

    fn close_all(&self) -> rusqlite::Result<()> {
        let mut accessor = self.lock.lock().expect("lock poisoning");
        while let Some(c) = accessor.pop() {
            if let Err((c, e)) = c.close() {
                accessor.push(c);
                return Err(e);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn get_open_connection_count(&self) -> usize {
        self.lock.lock().expect("lock poisoning").len()
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
