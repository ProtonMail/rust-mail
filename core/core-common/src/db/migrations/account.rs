//! Migrations for account based table information.
use include_dir::{Dir, include_dir};
use proton_sqlite3::{Migration, Migrator, MigratorError, file::embedded_migrations};
use stash::stash::Stash;

/// Migrate the accounts database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_account_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_account_version";
    static DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/db/migrations/account");
    let mut migrations: Vec<Box<dyn Migration>> = embedded_migrations(&DIR);

    let mut tether = stash.connection();
    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &mut migrations)
        .await
}
