//! Mapping of Mail domain into a Sqlite Database.

mod attachments;
mod conversations;
mod events;
pub mod json;
mod labels;
pub mod migrations;
mod settings;

pub use labels::*;

use proton_sqlite3::MigratorError;

pub type DBMigrationError = MigratorError;
pub use proton_sqlite3;

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
        proton_api_mail::domain::MessageFlags,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::proton_api_core::domain::AddressId,
        proton_api_mail::proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
}
