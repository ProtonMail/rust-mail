//! Migrations for core data types.
use include_dir::{include_dir, Dir};
use proton_sqlite3::{file::embedded_migrations, Migration, Migrator, MigratorError};
use stash::stash::Stash;

/// Migrate the core database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_core_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_core_version";
    static DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/db/migrations/core");
    let mut migrations: Vec<Box<dyn Migration>> = embedded_migrations(&DIR);

    let mut tether = stash.connection();
    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &mut migrations)
        .await
}
