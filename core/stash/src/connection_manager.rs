use parking_lot::Mutex;
pub use rusqlite;
use rusqlite::{Connection, Error, OpenFlags};
use sqlite_watcher::connection::State;
use sqlite_watcher::statement::Statement;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tempdir::TempDir;

#[derive(Debug)]
enum Source {
    File(PathBuf),
    TmpFile(TempDir),
}

type InitFn = dyn Fn(&mut Connection) -> Result<(), Error> + Send + Sync + 'static;

/// Maintains a pool of sqlite connections.
pub struct StashConnectionPool {
    connections: Mutex<Vec<Connection>>,
    source: Source,
    max_connections: usize,
    init_fn: Box<InitFn>,
    flags: OpenFlags,
}

impl StashConnectionPool {
    /// Creates a new `StashConnectionPool` from file.
    ///
    /// See `rusqlite::Connection::open`
    pub fn file<P: Into<PathBuf>>(
        path: P,
        max_connections: usize,
        init_fn: Box<InitFn>,
    ) -> Arc<Self> {
        Arc::new(Self {
            connections: Default::default(),
            source: Source::File(path.into()),
            max_connections,
            init_fn,
            flags: Default::default(),
        })
    }

    /// Creates a new `StashConnectionPool` pretending to be memory database.
    /// Due to many issues with shared_cache option and many more without, decision was made
    /// to build temp file databases and keep them alive in Manager context.
    /// This allows for flexibility of memory database and stability of file database in nice wrapping.
    /// Since the production usage is exclusively file database it is nice bonus to run all tests in the
    /// file.
    ///
    pub fn tmp_file(max_connections: usize, init_fn: Box<InitFn>) -> Arc<Self> {
        let tmp_dir = TempDir::new("stash-tmp").expect("failed to create temp dir");
        Arc::new(Self {
            connections: Default::default(),
            source: Source::TmpFile(tmp_dir),
            max_connections,
            init_fn,
            flags: Default::default(),
        })
    }

    /// Acquire a new connection from the pool.
    ///
    /// If connections are available in the pool we use one of those, otherwise we create a new one.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to initialize a connection.
    pub fn acquire(self: &Arc<Self>) -> Result<StashPooledConnection, Error> {
        self.create_or_acquire()
            .map(|c| StashPooledConnection::new(c, Arc::downgrade(self)))
    }

    fn create_or_acquire(&self) -> Result<Connection, Error> {
        let mut connections = self.connections.lock();
        while let Some(connection) = connections.pop() {
            if Self::is_connection_valid(&connection) {
                return Ok(connection);
            }
        }
        drop(connections);

        self.create_connection()
    }

    fn is_connection_valid(connection: &Connection) -> bool {
        connection.execute_batch("SELECT 1").is_ok()
    }

    fn create_connection(&self) -> Result<Connection, Error> {
        match &self.source {
            Source::File(path) => Connection::open_with_flags(path, self.flags),
            Source::TmpFile(tmp) => {
                Connection::open_with_flags(tmp.path().join("test"), self.flags)
            }
        }
        .and_then(|mut c| {
            (self.init_fn)(&mut c)?;
            State::set_pragmas().execute(&c)?;
            State::start_tracking().execute(&c)?;
            Ok(c)
        })
    }

    /// Release a connection back to the pool.
    fn release(&self, connection: Connection) {
        let mut connections = self.connections.lock();
        if connections.len() < self.max_connections {
            connections.push(connection);
        }
    }
}

/// A Sqlite Connection managed by the [`StashConnectionPool`].
///
/// On drop the connection is returned to the pool.
pub struct StashPooledConnection {
    conn: Option<Connection>,
    pool: Weak<StashConnectionPool>,
}

impl StashPooledConnection {
    fn new(connection: Connection, pool: Weak<StashConnectionPool>) -> Self {
        Self {
            conn: Some(connection),
            pool,
        }
    }
}

impl Drop for StashPooledConnection {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.release(self.conn.take().expect("Should be set"));
        }
    }
}

impl Deref for StashPooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().expect("Should be set")
    }
}

impl DerefMut for StashPooledConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut().expect("Should be set")
    }
}
