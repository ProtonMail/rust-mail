//! Migrations for the data model.
use proton_sqlite3::{Migration, Migrator, MigratorError, SqliteConnection};

mod v0;

const VERSION_TABLE_NAME: &str = "proton_mail_db_version";

pub fn migrate_db(conn: &mut SqliteConnection) -> Result<usize, MigratorError> {
    let migrations: Vec<Box<dyn Migration>> = vec![Box::new(v0::MigrationV0 {})];

    let migrator = Migrator::new();
    migrator.migrate(conn, VERSION_TABLE_NAME, &migrations)
}

#[test]
fn test_migration_on_empty_data_set() {
    let pool =
        proton_sqlite3::SqliteConnectionPool::new(proton_sqlite3::SqliteMode::InMemory, true);
    let mut conn = pool.acquire().expect("failed to acquire connection");
    migrate_db(&mut conn).expect("failed to migrate");
}
