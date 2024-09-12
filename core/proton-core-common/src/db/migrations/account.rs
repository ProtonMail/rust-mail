//! Migrations for account based table information.
use crate::db::migrations::account::v0::V0;
use proton_sqlite3::{Migrator, MigratorError};
use stash::stash::Stash;

pub mod v0;

/// Migrate the accounts database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_account_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_account_version";
    let migrations = vec![V0 {}];

    let migrator = Migrator::new();
    migrator
        .migrate(stash, VERSION_TABLE_NAME, &migrations)
        .await
}
