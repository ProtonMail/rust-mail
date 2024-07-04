//! Migrations for session based table information.
use proton_sqlite3::{Migration, Migrator, MigratorError};
use stash::params;
use stash::stash::Stash;

pub mod v0;

/// Migrate the session database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_session_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_session_version";
    let migrations: Vec<Box<dyn Migration>> = params![v0::V0 {}];

    let migrator = Migrator::new();
    migrator
        .migrate(stash, VERSION_TABLE_NAME, &migrations)
        .await
}
