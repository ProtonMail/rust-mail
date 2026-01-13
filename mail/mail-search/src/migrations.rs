//! Database migrations for mail-search crate
//!
//! This module defines migrations for search-related database tables.
//!
//! ## Usage
//!
//! Applications that want to use mail-search functionality should call
//! `run()` after initializing their databse context:
//!
//! ```rust,ignore
//! // After creating MailContext and getting a user stash
//! proton_mail_search::migrations::run(&user_stash).await?;
//! ```
//!
//! ## Migration Versioning
//!
//! Mail-search uses a separate migration version table (`proton_mail_search_version`)
//! which allows it to maintain its own migration numbering sequence
//! independent of mail-common's migrations. This prevents version conflicts and maintains isolation.

use include_dir::{Dir, include_dir};
use proton_sqlite3::{Migrator, MigratorError, file::embedded_migrations};
use stash::stash::Stash;

/// Run search-related database migrations
///
/// Applications using mail-search should call this function after
/// initializing the user database.
pub async fn run(stash: &Stash) -> Result<usize, MigratorError> {
    const TABLE: &str = "proton_mail_search_version";
    const MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/migrations");

    let migrations = embedded_migrations(&MIGRATIONS);
    let mut tether = stash.connection().await?;

    Migrator::new(TABLE, migrations).migrate(&mut tether).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use stash::stash::StashConfiguration;

    #[tokio::test]
    async fn smoke() {
        let stash = Stash::new(StashConfiguration::test()).unwrap();
        run(&stash).await.unwrap();
    }
}
