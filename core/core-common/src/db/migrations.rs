use include_dir::{Dir, include_dir};
use proton_sqlite3::{Migration, Migrator, MigratorError, file::embedded_migrations};
use stash::stash::Stash;

#[cfg(test)]
#[path = "../tests/db/migrations.rs"]
mod tests;

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

/// Migrate the core database.
///
/// # Errors
/// Returns error if the migration failed.
pub async fn migrate_core_db(stash: &Stash) -> Result<usize, MigratorError> {
    const VERSION_TABLE_NAME: &str = "proton_core_version";
    static DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/db/migrations/core");
    let mut migrations: Vec<Box<dyn Migration>> = embedded_migrations(&DIR);

    let mut tether = stash.connection();

    // Create action queue tables first as we can have items that depend on this.
    // This is safe to call multiple times as the migration code guarantees that
    // this will only run once per new version.
    proton_action_queue::db::create_tables(&mut tether).await?;

    let migrator = Migrator::new();
    migrator
        .migrate(&mut tether, VERSION_TABLE_NAME, &mut migrations)
        .await
}
