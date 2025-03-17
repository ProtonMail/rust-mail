//! Migrations for account based table information.
use crate::db::migrations::account::v0::V0;
use proton_sqlite3::{Migration, Migrator, MigratorError};
use stash::stash::Stash;

pub mod v0;

/// Migrate the accounts database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_account_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_account_version";
    let mut migrations: Vec<Box<dyn Migration>> = vec![Box::new(V0)];

    let mut tether = stash.connection();
    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &mut migrations)
        .await
}
