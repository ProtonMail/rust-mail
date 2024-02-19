//! Mapping of Mail domain into a Sqlite Database.

pub mod json;
mod labels;
mod migrations;
mod state;
mod uuid;

pub use labels::*;
pub use state::*;

use proton_sqlite3::rusqlite::Transaction;
use proton_sqlite3::{
    MigratorError, SqliteConnection, SqliteConnectionPool, SqliteMode, SqliteWatcher,
    SqliteWatcherError, SqliteWatcherHandler,
};
use std::ops::{Deref, DerefMut};

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;

/// Convenience wrapper around [`SqliteConnectionPool`] which always creates [`MailSqliteConnection`]
/// rather than the default [`SqliteConnection`].
pub struct MailSqliteConnectionPool(SqliteConnectionPool);

impl MailSqliteConnectionPool {
    pub fn new(mode: SqliteMode, debug: bool) -> Result<Self, MigratorError> {
        let pool = SqliteConnectionPool::new(mode, debug);
        let mut conn = pool.acquire()?;
        migrations::migrate_db(&mut conn)?;
        Ok(Self(pool))
    }

    /// Same as [`SqliteConnectionPool::acquire`].
    pub fn acquire(&self) -> DBResult<MailSqliteConnection> {
        self.0.acquire().map(MailSqliteConnection)
    }

    /// Same as [`SqliteConnectionPool::watch`].
    pub fn watch<T: SqliteWatcherHandler>(
        &self,
        handler: T,
    ) -> Result<SqliteWatcher, SqliteWatcherError> {
        self.0.watch(handler)
    }
}

/// This type provides access to all the required features to access and manipulate the data of
/// the mail domain. To access the feature set please use [`MailSqliteConnectionImpl`].
pub struct MailSqliteConnection(pub(crate) SqliteConnection);

impl MailSqliteConnection {
    pub fn new(conn: SqliteConnection) -> Self {
        Self(conn)
    }

    /// Get access to read only connection implementations.
    pub fn as_connection_ref(&self) -> MailSqliteConnectionRef<'_> {
        MailSqliteConnectionRef(MailSqliteConnectionImpl::new(self.0.deref()))
    }

    /// Create a new transaction.
    pub fn tx(&mut self) -> DBResult<MailSqliteTransaction<'_>> {
        self.0.transaction().map(MailSqliteTransaction::new)
    }
}

/// Connection implementation, all changes should be implemented targeting this type, so that the
/// same code can be used with regular connections or transactions and to enforce that the
/// mutable methods can only be accessed via transactions.
pub struct MailSqliteConnectionImpl<'c>(pub(crate) &'c proton_sqlite3::rusqlite::Connection);

impl<'c> MailSqliteConnectionImpl<'c> {
    fn new(conn: &'c proton_sqlite3::rusqlite::Connection) -> Self {
        Self(conn)
    }
}

/// Wrapper to promote read only access to [`MailSqliteConnectionImpl`].
pub struct MailSqliteConnectionRef<'c>(MailSqliteConnectionImpl<'c>);

impl<'c> Deref for MailSqliteConnectionRef<'c> {
    type Target = MailSqliteConnectionImpl<'c>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Wrapper to promote read and write access to [`MailSqliteConnectionImpl`].
pub struct MailSqliteConnectionMut<'c>(MailSqliteConnectionImpl<'c>);

impl<'c> Deref for MailSqliteConnectionMut<'c> {
    type Target = MailSqliteConnectionImpl<'c>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'c> DerefMut for MailSqliteConnectionMut<'c> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Mail Transaction, if not committed will rollback.
pub struct MailSqliteTransaction<'t>(pub(crate) Transaction<'t>);

impl<'t> MailSqliteTransaction<'t> {
    fn new(tx: Transaction<'t>) -> Self {
        Self(tx)
    }

    /// Get access to the connection reference.
    pub fn as_connection_mut(&mut self) -> MailSqliteConnectionMut<'_> {
        MailSqliteConnectionMut(MailSqliteConnectionImpl::new(self.0.deref()))
    }

    /// Commit the transaction.
    pub fn commit(self) -> DBResult<()> {
        self.0.commit()
    }
}

#[cfg(test)]
pub(crate) fn new_test_connection() -> (
    MailSqliteConnection,
    MailSqliteConnectionPool,
    proton_api_mail::proton_api_core::exports::tracing::subscriber::DefaultGuard,
) {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(proton_api_mail::proton_api_core::exports::tracing::Level::TRACE)
        // completes the builder.
        .finish();

    let guard =
        proton_api_mail::proton_api_core::exports::tracing::subscriber::set_default(subscriber);
    let pool =
        MailSqliteConnectionPool::new(SqliteMode::InMemory, true).expect("failed to create pool");
    let conn = pool.acquire().expect("failed to acquire connection");
    (conn, pool, guard)
}
