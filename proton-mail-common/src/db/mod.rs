//! Mapping of Mail domain into a Sqlite Database.

mod attachments;
mod conversations;
mod events;
mod ids;
pub mod json;
mod labels;
pub mod migrations;
mod settings;

pub use attachments::*;
pub use conversations::*;
pub use labels::*;
pub use settings::*;

use proton_sqlite3::{
    new_tracked_connection_wrapper, InProcessTrackerService, MigratorError, TrackingConnection,
};
use std::ops::Deref;

#[cfg(test)]
use proton_core_common::db::{CoreSqliteConnection, CoreSqliteConnectionMut};

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;
pub type DBMigrationError = MigratorError;
pub use proton_sqlite3;

new_tracked_connection_wrapper!(MailSqliteConnection);

/// Convenience wrapper around [`InProcessTrackerService`] which always creates [`MailSqliteConnection`]
/// rather than the default [`proton_sqlite3::SqliteConnection`].
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

    /// Same as [`proton_sqlite3::SqliteConnectionPool::acquire`].
    pub fn acquire(&self) -> DBResult<MailSqliteConnection> {
        let conn = self.0.db_pool().acquire()?;
        let conn = TrackingConnection::new(conn, self.0.clone())?;
        Ok(MailSqliteConnection(conn))
    }
}

#[cfg(test)]
pub(crate) fn new_test_connection() -> (
    CoreSqliteConnection,
    MailSqliteConnection,
    MailSqliteConnectionPool,
) {
    use proton_sqlite3::{SqliteConnectionPool, SqliteMode};

    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, true);
    let service = InProcessTrackerService::new(pool).expect("failed to create tracker service");
    {
        let mut conn = service.db_pool().acquire().unwrap();
        proton_core_common::db::migrate_core_db(&mut conn).unwrap();
    }
    let core_conn: CoreSqliteConnection = service
        .new_connection()
        .expect("failed to acquire connection")
        .into();
    let pool = MailSqliteConnectionPool::new(service).expect("failed to create pool");
    let conn = pool.acquire().expect("failed to acquire connection");
    (core_conn, conn, pool)
}

#[cfg(test)]
pub(crate) fn with_file_sqlite_db(
    f: impl Fn(CoreSqliteConnection, MailSqliteConnection, MailSqliteConnectionPool),
) {
    use proton_sqlite3::{SqliteConnectionPool, SqliteMode};
    let db_dir = tempfile::tempdir().unwrap();

    let pool = SqliteConnectionPool::new(SqliteMode::File(db_dir.path().join("test")), true);
    let service = InProcessTrackerService::new(pool).expect("failed to create tracker service");
    {
        let mut conn = service.db_pool().acquire().unwrap();
        proton_core_common::db::migrate_core_db(&mut conn).unwrap();
    }
    let core_conn: CoreSqliteConnection = service
        .new_connection()
        .expect("failed to acquire connection")
        .into();
    let pool = MailSqliteConnectionPool::new(service).expect("failed to create pool");
    let conn = pool.acquire().expect("failed to acquire connection");
    f(core_conn, conn, pool);
    // Check that the temporary dir removal works.
    db_dir.close().unwrap();
}

#[cfg(test)]
pub(crate) fn with_tx(conn: &mut MailSqliteConnection, f: impl Fn(&mut MailSqliteConnectionMut)) {
    conn.tx(move |tx| -> DBResult<()> {
        (f)(tx);
        Ok(())
    })
    .expect("failed transaction");
}

#[cfg(test)]
pub(crate) fn with_tx_core(
    conn: &mut CoreSqliteConnection,
    f: impl Fn(&mut CoreSqliteConnectionMut),
) {
    conn.tx(move |tx| -> DBResult<()> {
        (f)(tx);
        Ok(())
    })
    .expect("failed transaction");
}

#[cfg(feature = "uniffi")]
mod type_forwarding {
    // Required due to https://github.com/mozilla/uniffi-rs/issues/1988.

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::ConversationId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::AttachmentId,
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

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::ExternalId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::proton_api_core::domain::AddressId,
        proton_api_mail::proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
}
