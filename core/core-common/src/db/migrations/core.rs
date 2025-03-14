//! Migrations for core data types.
use crate::db::migrations::core::v0::V0;
use proton_sqlite3::{Migration, Migrator, MigratorError};
use stash::stash::Stash;

pub mod v0;

/// Migrate the core database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_core_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_core_version";
    let mut migrations: Vec<Box<dyn Migration>> = vec![Box::new(V0)];

    let mut tether = stash.connection();
    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &mut migrations)
        .await
}
