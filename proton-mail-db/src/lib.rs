//! Mapping of Mail domain into a Sqlite Database.

mod addresses;
mod attachments;
mod conversations;
mod ids;
pub mod json;
mod labels;
mod migrations;
mod state;

pub use attachments::*;
pub use conversations::*;
pub use labels::*;
pub use state::*;

use proton_sqlite3::{InProcessTrackerService, MigratorError, TrackingConnection};
use std::ops::{Deref, DerefMut};

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;
pub type DBMigrationError = MigratorError;

/// Convenience wrapper around [`InProcessTrackerService`] which always creates [`MailSqliteConnection`]
/// rather than the default [`SqliteConnection`].
#[derive(Clone)]
pub struct MailSqliteConnectionPool(InProcessTrackerService);

impl MailSqliteConnectionPool {
    pub fn new(service: InProcessTrackerService) -> Result<Self, MigratorError> {
        let mut conn = service.db_pool().acquire()?;
        migrations::migrate_db(&mut conn)?;
        Ok(Self(service))
    }

    pub fn tracker_service(&self) -> &InProcessTrackerService {
        &self.0
    }

    /// Same as [`SqliteConnectionPool::acquire`].
    pub fn acquire(&self) -> DBResult<MailSqliteConnection> {
        let conn = self.0.db_pool().acquire()?;
        let conn = TrackingConnection::new(conn, self.0.clone())?;
        Ok(MailSqliteConnection(conn))
    }
}

/// This type provides access to all the required features to access and manipulate the data of
/// the mail domain. To access the feature set please use [`MailSqliteConnectionImpl`].
pub struct MailSqliteConnection(pub(crate) TrackingConnection);

impl MailSqliteConnection {
    pub fn new(conn: TrackingConnection) -> Self {
        Self(conn)
    }

    /// Get access to read only connection implementations.
    pub fn as_connection_ref(&self) -> MailSqliteConnectionRef<'_> {
        MailSqliteConnectionRef(MailSqliteConnectionImpl::new(self.0.as_ref()))
    }

    /// Create a new transaction.
    pub fn tx<T, E: From<DBError>>(
        &mut self,
        mut closure: impl FnMut(&mut MailSqliteConnectionMut) -> Result<T, E>,
    ) -> Result<T, E> {
        self.0.tx(|tx| {
            let conn_impl = MailSqliteConnectionImpl(tx.deref());
            let mut conn = MailSqliteConnectionMut(conn_impl);
            closure(&mut conn)
        })
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

#[cfg(test)]
pub(crate) fn new_test_connection() -> (
    MailSqliteConnection,
    MailSqliteConnectionPool,
    proton_api_mail::proton_api_core::exports::tracing::subscriber::DefaultGuard,
) {
    use proton_sqlite3::{SqliteConnectionPool, SqliteMode};

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(proton_api_mail::proton_api_core::exports::tracing::Level::TRACE)
        // completes the builder.
        .finish();

    let guard =
        proton_api_mail::proton_api_core::exports::tracing::subscriber::set_default(subscriber);

    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, true);
    let service = InProcessTrackerService::new(pool);

    let pool = MailSqliteConnectionPool::new(service).expect("failed to create pool");
    let conn = pool.acquire().expect("failed to acquire connection");
    (conn, pool, guard)
}

#[cfg(test)]
pub(crate) fn with_tx(conn: &mut MailSqliteConnection, f: impl Fn(&mut MailSqliteConnectionMut)) {
    conn.tx(move |tx| -> DBResult<()> {
        (f)(tx);
        Ok(())
    })
    .expect("failed transaction");
}
