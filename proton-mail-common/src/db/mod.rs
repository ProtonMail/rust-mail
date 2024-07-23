//! Mapping of Mail domain into a Sqlite Database.

mod attachments;
mod conversations;
mod events;
pub mod json;
mod labels;
pub mod migrations;
mod settings;
pub type DBMigrationError = MigratorError;

pub use proton_sqlite3;

#[cfg(test)]
use crate::db::migrations::migrate_db;
#[cfg(test)]
use proton_core_common::db::migrations::migrate_core_db;
use proton_sqlite3::migration::MigratorError;
#[cfg(test)]
use stash::stash::Stash;
#[cfg(test)]
use tempfile::{tempdir, TempDir};

#[cfg(test)]
pub(crate) async fn new_test_connection() -> Stash {
    use std::io::stdout;
    use tracing::subscriber::set_global_default;
    use tracing::Level;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{registry, EnvFilter};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));

    let stash = Stash::new(None).expect("failed to create stash");
    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.expect("failed to migrate");
    stash
}

#[cfg(test)]
pub(crate) async fn new_test_connection_file() -> (Stash, TempDir) {
    use std::io::stdout;
    use tracing::subscriber::set_global_default;
    use tracing::Level;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{registry, EnvFilter};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));

    let db_dir = tempdir().unwrap();
    let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("failed to create stash");
    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.expect("failed to migrate");
    (stash, db_dir)
}
