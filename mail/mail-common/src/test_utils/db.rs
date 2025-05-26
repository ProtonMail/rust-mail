use crate::db::migrations::migrate_db;
use proton_core_common::db::migrations::migrate_core_db;
use stash::stash::{Stash, StashConfiguration};
use tempfile::{TempDir, tempdir};

/// # Panics
pub async fn new_test_connection() -> Stash {
    use std::io::stdout;
    use tracing::Level;
    use tracing::subscriber::set_global_default;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{EnvFilter, registry};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));

    let stash = Stash::new(StashConfiguration::test()).expect("failed to create stash");
    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.expect("failed to migrate");
    // We need the action queue table due to message delete triggering
    // some foreign key constrains in draft metadata that relate to the action queue
    // table.
    let _ = proton_action_queue::queue::Queue::new(stash.clone())
        .await
        .unwrap();
    stash
}

/// # Panics
pub async fn new_test_connection_file() -> (Stash, TempDir) {
    use std::io::stdout;
    use tracing::Level;
    use tracing::subscriber::set_global_default;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{EnvFilter, registry};
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));

    let db_dir = tempdir().unwrap();
    let stash = Stash::new(StashConfiguration::test_with_path(
        &db_dir.path().join("test"),
    ))
    .expect("failed to create stash");
    migrate_core_db(&stash).await.unwrap();
    migrate_db(&stash).await.expect("failed to migrate");
    // We need the action queue table due to message delete triggering
    // some foreign key constrains in draft metadata that relate to the action queue
    // table.
    let _ = proton_action_queue::queue::Queue::new(stash.clone())
        .await
        .unwrap();
    (stash, db_dir)
}
