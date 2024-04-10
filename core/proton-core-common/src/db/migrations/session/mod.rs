//! Migrations for session based table information.
use proton_sqlite3::{Migration, Migrator, MigratorError, SqliteConnection};

pub mod v0;

pub fn migrate_session_db(conn: &mut SqliteConnection) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_session_version";
    let migrations: Vec<Box<dyn Migration>> = vec![Box::new(v0::SessionMigrationV0 {})];

    let migrator = Migrator::new();
    migrator.migrate(conn, VERSION_TABLE_NAME, &migrations)
}

#[test]
fn test_core_migration_on_empty_data_set() {
    let pool =
        proton_sqlite3::SqliteConnectionPool::new(proton_sqlite3::SqliteMode::InMemory, true);
    let mut conn = pool.acquire().expect("failed to acquire connection");
    migrate_session_db(&mut conn).expect("failed to migrate");
}
