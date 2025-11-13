use crate::db::migrations::migrate_db;
use proton_core_common::db::migrations::migrate_core_db;
use stash::stash::{Stash, StashConfiguration};
use tempfile::{TempDir, tempdir};
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, registry};

pub async fn new_test_connection() -> Stash {
    _ = set_global_default(
        registry()
            .with(EnvFilter::new("debug"))
            .with(layer().with_test_writer()),
    );

    let stash = Stash::new(StashConfiguration::test()).unwrap();

    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.unwrap();

    stash
}

pub async fn new_test_connection_file() -> (Stash, TempDir) {
    _ = set_global_default(
        registry()
            .with(EnvFilter::new("debug"))
            .with(layer().with_test_writer()),
    );

    let db_dir = tempdir().unwrap();

    let stash = Stash::new(StashConfiguration::test_with_path(
        &db_dir.path().join("test"),
    ))
    .unwrap();

    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.unwrap();

    (stash, db_dir)
}
