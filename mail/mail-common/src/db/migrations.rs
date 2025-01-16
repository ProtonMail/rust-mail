//! Migrations for the data model.
use crate::db::migrations::v0::MigrationV0;
use proton_sqlite3::{Migrator, MigratorError};
use stash::stash::Stash;

mod v0;

const VERSION_TABLE_NAME: &str = "proton_mail_db_version";

pub async fn migrate_db(stash: &Stash) -> Result<usize, MigratorError> {
    let migrations = vec![MigrationV0 {}];
    let mut tether = stash.connection();
    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &migrations)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_common::db::migrations::migrate_core_db;

    #[tokio::test]
    async fn test_migration_on_empty_data_set() {
        let stash = Stash::new(None).expect("Failed to create Stash");
        migrate_core_db(&stash).await.expect("failed to migrate");
        migrate_db(&stash).await.expect("failed to migrate");
    }
}
