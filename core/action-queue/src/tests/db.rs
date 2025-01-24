#![allow(non_snake_case)]

use super::*;
use crate::action::{DefaultVersionConverter, MetadataBuilder, Type};
use crate::tests::common::NoopActionHandler;
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use stash::{orm::Model, stash::Stash};

#[derive(Deserialize, Serialize, Eq, PartialEq)]
struct TestAction {
    bar: u32,
    foo: String,
}

#[derive(Debug, thiserror::Error)]
enum Error {}

impl action::Error for Error {
    fn is_network_failure(&self) -> bool {
        false
    }
}

impl Action for TestAction {
    const TYPE: Type = Type("test_action");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = NoopActionHandler<Self>;

    type RemoteOutput = ();
    type LocalOutput = ();

    type Error = Error;

    type Context = ();
}
#[tokio::test]
async fn db_migration() {
    new_test_connection().await;
}

#[tokio::test]
async fn action_store_and_retrieve() {
    let state = TestAction {
        foo: "foo".to_string(),
        bar: 2048,
    };

    let stash = new_test_connection().await;
    let mut conn = stash.connection();
    let mut stored = StoredAction::new::<TestAction>(&state, Metadata::default()).unwrap();

    let tx = conn.transaction().await.unwrap();
    stored.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let first_action_id = stored.id.unwrap();

    let metadata = MetadataBuilder::new()
        .with_debug_string("my debug string")
        .with_dependency(first_action_id)
        .with_resource(&"Resource")
        .unwrap()
        .build();

    let mut stored = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();

    let tx = conn.transaction().await.unwrap();
    stored.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let id = stored.id.unwrap();
    let db_action = StoredAction::load(id, &conn).await.unwrap().unwrap();

    assert_eq!(stored, db_action);

    // delete action should delete both actions
    let tx = conn.transaction().await.unwrap();
    StoredAction::delete(&tx, first_action_id).await.unwrap();
    tx.commit().await.unwrap();

    let remaining = StoredAction::pending_count(&conn).await.unwrap();

    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn action_replace_or_queue() {
    let state = TestAction {
        foo: "foo".to_string(),
        bar: 2048,
    };

    let stash = new_test_connection().await;
    let mut conn = stash.connection();
    let mut stored = StoredAction::new::<TestAction>(&state, Metadata::default()).unwrap();

    let tx = conn.transaction().await.unwrap();
    stored.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let first_action_id = stored.id.unwrap();

    let metadata = MetadataBuilder::new()
        .with_debug_string("my debug string")
        .with_dependency(first_action_id)
        .with_resource(&"Resource")
        .unwrap()
        .build();

    // Simulate same action update.
    let mut updated = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();
    let tx = conn.transaction().await.unwrap();
    updated
        .create_or_update(first_action_id, &tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(stored.id, updated.id);

    // Compare against db value
    let id = stored.id.unwrap();
    let db_action = StoredAction::load(id, &conn).await.unwrap().unwrap();
    assert_eq!(updated, db_action);

    // Simulate update with different type
    let mut updated = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();
    updated.action_type = "unknown_action_type".to_owned();
    let tx = conn.transaction().await.unwrap();
    updated
        .create_or_update(first_action_id, &tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(stored.id, updated.id);
}

async fn new_test_connection() -> Stash {
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
    let stash = Stash::new(None).unwrap();
    let mut tether = stash.connection();
    create_tables(&mut tether).await.unwrap();
    stash
}
