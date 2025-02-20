#![allow(non_snake_case)]

use super::*;
use crate::action::{
    ActionGroup, DefaultVersionConverter, MetadataBuilder, Type, WriterGuardError,
};
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
enum Error {
    #[error("Writer Guard Expired")]
    WriterGuardExpired,
    #[error("Other")]
    Other,
}

impl action::Error for Error {
    fn is_network_failure(&self) -> bool {
        false
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Error::WriterGuardExpired)
    }
}

impl From<WriterGuardError> for Error {
    fn from(m: WriterGuardError) -> Self {
        if matches!(m, WriterGuardError::Expired) {
            Self::WriterGuardExpired
        } else {
            Self::Other
        }
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
async fn action_execution_lock() {
    let state = TestAction {
        foo: "foo".to_string(),
        bar: 2048,
    };

    let stash = new_test_connection().await;
    let mut conn = stash.connection();
    let mut stored = StoredAction::new::<TestAction>(&state, Metadata::default()).unwrap();

    let tx = conn.transaction().await.unwrap();
    stored.save(&tx).await.unwrap();

    let first_action_id = stored.id.unwrap();

    let metadata = MetadataBuilder::new()
        .with_debug_string("my debug string")
        .with_dependency(first_action_id)
        .with_resource(&"Resource")
        .unwrap()
        .build();

    let mut stored = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();
    stored.save(&tx).await.unwrap();

    let second_action_id = stored.id.unwrap();

    let mut stored = StoredAction::new::<TestAction>(&state, Metadata::default()).unwrap();
    stored.save(&tx).await.unwrap();

    let third_action_id = stored.id.unwrap();

    tx.commit().await.unwrap();

    let tx = conn.transaction().await.unwrap();

    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), first_action_id);

    // Acquire lock
    let _ =
        ExecutionGuard::acquire_with_timestamp(first_action_id, "EXEC".to_owned(), Utc::now(), &tx)
            .await
            .unwrap();

    // Next action should be the third, since action 2 depends on action one.
    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), third_action_id);

    // Simulate timedout lock by setting timeout in the past.
    let _ = ExecutionGuard::acquire_with_timestamp(
        first_action_id,
        "EXEC2".to_owned(),
        Utc::now() - chrono::Duration::seconds(120),
        &tx,
    )
    .await
    .unwrap();

    // Next action should be the first, since the execution lock timed out.
    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), first_action_id);

    // Delete first action
    StoredAction::delete(&tx, first_action_id).await.unwrap();

    // Next action should be the second, since there is no execution lock
    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), second_action_id);

    // Acquire lock
    let _ = ExecutionGuard::acquire_with_timestamp(
        second_action_id,
        "EXEC".to_owned(),
        Utc::now(),
        &tx,
    )
    .await
    .unwrap();

    // We should now receive the last action.
    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), third_action_id);

    // Acquire lock for the 3rd action
    let lock = ExecutionGuard::acquire_with_timestamp(
        second_action_id,
        "EXEC2".to_owned(),
        Utc::now(),
        &tx,
    )
    .await
    .unwrap();

    // Release the lock on the 2nd action
    lock.release(&tx).await.unwrap();

    // We should receive the second action again.
    let next_action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next_action.id.unwrap(), second_action_id);
}

#[tokio::test]
async fn action_execution_group_selection() {
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

    let tx = conn.transaction().await.unwrap();
    // Action has default group, so it should show up.
    let action = StoredAction::next(ActionGroup::default().as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(action.id.unwrap(), stored.id.unwrap());

    // This group does not exist and no action are assigned to it.
    let unknown_group = ActionGroup::new("UNKNOWN");
    let action = StoredAction::next(unknown_group.as_ref(), &tx)
        .await
        .unwrap();
    assert!(action.is_none());

    // Save an action with this new group.
    let metadata = MetadataBuilder::new()
        .with_group_override(unknown_group.clone())
        .build();

    let mut stored = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();
    stored.save(&tx).await.unwrap();

    // We should now have an action.
    let action = StoredAction::next(unknown_group.as_ref(), &tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(action.id.unwrap(), stored.id.unwrap());
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
