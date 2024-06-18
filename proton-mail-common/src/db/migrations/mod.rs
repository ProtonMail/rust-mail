//! Migrations for the data model.
use proton_sqlite3::{Migration, Migrator, MigratorError};
use stash::stash::Stash;

mod v0;

const VERSION_TABLE_NAME: &str = "proton_mail_db_version";

pub async fn migrate_db(conn: &Stash) -> Result<usize, MigratorError> {
    let migrations: Vec<Box<dyn Migration>> = vec![Box::new(v0::MigrationV0 {})];

    let migrator = Migrator::new();
    migrator.migrate(conn, VERSION_TABLE_NAME, &migrations).await
}

#[tokio::test]
async fn test_migration_on_empty_data_set() {
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_db(&stash).await.expect("failed to migrate");
}
