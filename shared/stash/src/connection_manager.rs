use crate::stash::{PooledTether, PooledTetherInterruptNotifier};
use parking_lot::{Condvar, Mutex};
pub use rusqlite;
use rusqlite::{Connection, Error, InterruptHandle, OpenFlags};
use sqlite_watcher::connection::State;
use sqlite_watcher::statement::Statement;
use sqlite_watcher::watcher::Watcher;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[derive(Debug)]
enum Source {
    File(PathBuf),
    TmpFile(TempDir),
}

#[derive(Debug, thiserror::Error)]
pub enum StashConnectionPoolError {
    #[error(transparent)]
    Connection(#[from] Error),
    #[error("Failed to acquire a connection in the given time limit")]
    TimedOut,
}

type InitFn = dyn Fn(&mut Connection) -> Result<(), Error> + Send + Sync + 'static;
/// Maintains a pool of sqlite connections.
pub struct StashConnectionPool {
    connections: Mutex<Vec<PooledTether>>,
    connections_cond_var: Condvar,
    interrupts: Vec<InterruptData>,
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
        watcher: &Arc<Watcher>,
    ) -> Result<Arc<Self>, Error> {
        Self::new(Source::File(path.into()), max_connections, init_fn, watcher)
    }

    /// Creates a new `StashConnectionPool` pretending to be memory database.
    /// Due to many issues with shared_cache option and many more without, decision was made
    /// to build temp file databases and keep them alive in Manager context.
    /// This allows for flexibility of memory database and stability of file database in nice wrapping.
    /// Since the production usage is exclusively file database it is nice bonus to run all tests in the
    /// file.
    ///
    pub fn tmp_file(
        max_connections: usize,
        init_fn: Box<InitFn>,
        watcher: &Arc<Watcher>,
    ) -> Result<Arc<Self>, Error> {
        let tmp_dir = TempDir::new().expect("failed to create temp dir");
        Self::new(Source::TmpFile(tmp_dir), max_connections, init_fn, watcher)
    }

    fn new(
        source: Source,
        max_connections: usize,
        init_fn: Box<InitFn>,
        watcher: &Arc<Watcher>,
    ) -> Result<Arc<Self>, Error> {
        let connections: Result<Vec<Connection>, Error> = (0..max_connections)
            .map(|_| Self::create_connection(&source, &init_fn, OpenFlags::default()))
            .collect::<Vec<_>>()
            .into_iter()
            .collect();
        let connections = connections?;

        Ok(Arc::new_cyclic(|weak| {
            let mut interrupts = Vec::with_capacity(connections.len());
            let connections = connections
                .into_iter()
                .enumerate()
                .map(|(idx, conn)| {
                    let handle = conn.get_interrupt_handle();
                    let pooled_tether = PooledTether::new(conn, watcher, weak.clone(), idx);
                    interrupts.push(InterruptData {
                        handle,
                        interrupt_notifier: pooled_tether.interrupt_notifier(),
                    });
                    pooled_tether
                })
                .collect();
            Self {
                connections: Mutex::new(connections),
                connections_cond_var: Condvar::new(),
                interrupts,
                interrupted: Default::default(),
                wait_resume: Condvar::new(),
            }
        }))
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
        timeout: Option<Duration>,
    ) -> Result<StashPooledConnection, StashConnectionPoolError> {
        let connection = self.wait_or_acquire(timeout)?;
        Ok(StashPooledConnection::new(connection, self.clone()))
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
            for handle in &self.interrupts {
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

    fn wait_or_acquire(
        &self,
        timeout: Option<Duration>,
    ) -> Result<PooledTether, StashConnectionPoolError> {
        loop {
            let mut connections = self.connections.lock();
            if connections.is_empty() {
                if let Some(timeout) = timeout {
                    let result = self
                        .connections_cond_var
                        .wait_for(&mut connections, timeout);
                    if result.timed_out() {
                        return Err(StashConnectionPoolError::TimedOut);
                    }
                } else {
                    self.connections_cond_var.wait(&mut connections);
                }
            }

            if let Some(connection) = connections.pop() {
                return Ok(connection);
            }
        }
    }

    fn create_connection(
        source: &Source,
        init_fn: &InitFn,
        flags: OpenFlags,
    ) -> Result<Connection, Error> {
        match source {
            Source::File(path) => Connection::open_with_flags(path, flags),
            Source::TmpFile(tmp) => Connection::open_with_flags(tmp.path().join("test"), flags),
        }
        .and_then(|mut c| {
            (init_fn)(&mut c)?;
            State::set_pragmas().execute(&c)?;
            State::start_tracking().execute(&c)?;
            Ok(c)
        })
    }

    /// Release a connection back to the pool.
    fn release(&self, connection: PooledTether) {
        let mut connections = self.connections.lock();
        connections.push(connection);
        self.connections_cond_var.notify_one();
    }
}

/// A Sqlite Connection managed by the [`StashConnectionPool`].
///
/// On drop the connection is returned to the pool.
pub(crate) struct StashPooledConnection {
    conn: Option<PooledTether>,
    pool: Arc<StashConnectionPool>,
}

impl StashPooledConnection {
    fn new(connection: PooledTether, pool: Arc<StashConnectionPool>) -> Self {
        Self {
            conn: Some(connection),
            pool,
        }
    }
}

impl Drop for StashPooledConnection {
    fn drop(&mut self) {
        self.pool.release(self.conn.take().expect("Should be set"));
    }
}

impl Deref for StashPooledConnection {
    type Target = PooledTether;
    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().expect("Should be set")
    }
}

impl DerefMut for StashPooledConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn.as_mut().expect("Should be set")
    }
}

struct InterruptData {
    handle: InterruptHandle,
    interrupt_notifier: PooledTetherInterruptNotifier,
}

impl InterruptData {
    fn interrupt(&self) {
        self.handle.interrupt();
        self.interrupt_notifier.interrupt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_pool_upper_limit() {
        let watcher = Watcher::new().unwrap();
        let pool = StashConnectionPool::tmp_file(2, Box::new(|_| Ok(())), &watcher).unwrap();

        let conn1 = pool.acquire(None).unwrap();
        let _conn2 = pool.acquire(None).unwrap();
        let r = pool.acquire(Some(Duration::from_millis(200)));
        assert!(matches!(r, Err(StashConnectionPoolError::TimedOut)));
        drop(conn1);
        pool.acquire(Some(Duration::from_millis(200))).unwrap();
    }

    #[test]
    fn connection_pool_waits_on_connections_to_be_returned() {
        let watcher = Watcher::new().unwrap();
        let pool = StashConnectionPool::tmp_file(2, Box::new(|_| Ok(())), &watcher).unwrap();

        let conn1 = pool.acquire(None).unwrap();
        let _conn2 = pool.acquire(None).unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(conn1);
        });
        pool.acquire(Some(Duration::from_secs(1))).unwrap();
    }
}
