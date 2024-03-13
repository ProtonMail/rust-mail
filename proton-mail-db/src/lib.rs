//! Mapping of Mail domain into a Sqlite Database.

mod addresses;
mod attachments;
mod conversations;
mod events;
mod ids;
pub mod json;
mod labels;
pub mod migrations;
mod settings;
mod state;

pub use attachments::*;
pub use conversations::*;
pub use labels::*;
pub use state::*;

use proton_sqlite3::{
    new_tracked_connection_wrapper, InProcessTrackerService, MigratorError, TrackingConnection,
};
use std::ops::Deref;

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;
pub type DBMigrationError = MigratorError;
pub use proton_sqlite3;

new_tracked_connection_wrapper!(MailSqliteConnection);

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
    let service = InProcessTrackerService::new(pool).expect("failed to create tracker service");

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

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
mod type_forwarding {
    // Required due to https://github.com/mozilla/uniffi-rs/issues/1988.

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::ConversationId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::LabelId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::MessageId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );
}
