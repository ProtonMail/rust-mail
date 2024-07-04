#![allow(non_snake_case)]

use stash::stash::Stash;

pub async fn new_core_test_connection() -> Stash {
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
    use crate::db::migrations::migrate_core_db;
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_core_db(&stash).await.unwrap();
    stash
}
