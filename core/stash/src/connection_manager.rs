use parking_lot::{Condvar, Mutex};
pub use rusqlite;
use rusqlite::{Connection, Error, InterruptHandle, OpenFlags};
use slotmap::SlotMap;
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
type NotifierFn = dyn Fn() + Send + Sync + 'static;
/// Maintains a pool of sqlite connections.
pub struct StashConnectionPool {
    connections: Mutex<Vec<Connection>>,
    interrupt_handles: Mutex<SlotMap<InterruptHandleKey, InterruptData>>,
    source: Source,
    max_connections: usize,
    init_fn: Box<InitFn>,
    flags: OpenFlags,
    interrupted: Mutex<bool>,
    wait_resume: Condvar,
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
            interrupt_handles: Default::default(),
            source: Source::File(path.into()),
            max_connections,
            init_fn,
            flags: Default::default(),
            interrupted: Default::default(),
            wait_resume: Condvar::new(),
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
            interrupt_handles: Default::default(),
            source: Source::TmpFile(tmp_dir),
            max_connections,
            init_fn,
            flags: Default::default(),
            interrupted: Default::default(),
            wait_resume: Condvar::new(),
        })
    }

    /// Acquire a new connection from the pool.
    ///
    /// `notify_interrupt` is required so that all active connections can be notified of a request
    /// to interrupt the exeuction of sql code.
    ///
    /// If connections are available in the pool we use one of those, otherwise we create a new one.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to initialize a connection.
    pub fn acquire(
        self: &Arc<Self>,
        notify_interrupt: Box<NotifierFn>,
    ) -> Result<StashPooledConnection, Error> {
        let connection = self.create_or_acquire()?;

        let mut interrupt_handles = self.interrupt_handles.lock();
        let key = interrupt_handles.insert(InterruptData {
            handle: connection.get_interrupt_handle(),
            notifier: notify_interrupt,
        });
        drop(interrupt_handles);

        Ok(StashPooledConnection::new(
            connection,
            key,
            Arc::downgrade(self),
        ))
    }

    /// Interrupt all ongoing queries and rollback any active transactions.
    ///
    /// This method is useful on iOS to ensure that the db file locks are released and no new
    /// transaction is started until `resume()` is called.
    pub fn interrupt(&self) {
        let was_interrupted = {
            let mut interrupted = self.interrupted.lock();
            let old_value = *interrupted;
            *interrupted = true;
            drop(interrupted);
            old_value
        };

        // interrupt all connections
        if !was_interrupted {
            tracing::info!("Interrupting stash");
            let interrupt_handles = self.interrupt_handles.lock();
            for handle in interrupt_handles.values() {
                handle.interrupt()
            }
        }
    }

    /// Resume execution and allow the tethers to proceed.
    pub fn resume(&self) {
        tracing::info!("Resuming stash");
        let mut is_interrupted = self.interrupted.lock();
        *is_interrupted = false;
        self.wait_resume.notify_all();
    }

    /// Check whether we can proceed with new sql queries or wait on the user to call `resume()`.
    pub fn check_interrupted_or_wait_resume(&self) {
        let mut interrupted = self.interrupted.lock();
        if !*interrupted {
            return;
        }
        tracing::info!("Stash is interrupted, waiting on resume");
        self.wait_resume.wait(&mut interrupted);
    }

    pub fn create_or_acquire(&self) -> Result<Connection, Error> {
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
    fn release(&self, connection: Connection, interrupt_handle_key: InterruptHandleKey) {
        let mut interrupt_handles = self.interrupt_handles.lock();
        interrupt_handles.remove(interrupt_handle_key);
        drop(interrupt_handles);
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
    interrupt_handle_key: InterruptHandleKey,
}

impl StashPooledConnection {
    fn new(
        connection: Connection,
        interrupt_handle_key: InterruptHandleKey,
        pool: Weak<StashConnectionPool>,
    ) -> Self {
        Self {
            conn: Some(connection),
            pool,
            interrupt_handle_key,
        }
    }
}

impl Drop for StashPooledConnection {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.release(
                self.conn.take().expect("Should be set"),
                self.interrupt_handle_key,
            );
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

slotmap::new_key_type! {
    pub(crate) struct InterruptHandleKey;
}

struct InterruptData {
    handle: InterruptHandle,
    notifier: Box<NotifierFn>,
}

impl InterruptData {
    fn interrupt(&self) {
        self.handle.interrupt();
        (self.notifier)();
    }
}
