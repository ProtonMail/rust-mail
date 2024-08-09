#![allow(non_snake_case)]

use super::*;
use crate::tests::common::NoopActionHandler;
use proton_api_core::service::ApiServiceError;
use stash::orm::Model;
use stash::stash::Interface;

#[tokio::test]
async fn db_migration() {
    new_test_connection().await;
}

#[tokio::test]
async fn action_store_and_retrieve() {
    use crate::action::{Action, DefaultVersionConverter, Metadata, MetadataBuilder, Type};
    use serde::{Deserialize, Serialize};
    #[derive(Deserialize, Serialize, Eq, PartialEq)]
    struct TestAction {
        bar: u32,
        foo: String,
    }

    #[derive(Debug, thiserror::Error)]
    enum Error {}

    impl action::Error for Error {
        fn request_error(&self) -> Option<&ApiServiceError> {
            None
        }
    }

    impl Action for TestAction {
        const TYPE: Type = Type("test_action");
        const VERSION: u32 = 1;
        type VersionConverter = DefaultVersionConverter<Self>;
        type Output = ();
        type Error = Error;

        type Handler = NoopActionHandler<Self>;
    }

    let state = TestAction {
        foo: "foo".to_string(),
        bar: 2048,
    };

    let stash = new_test_connection().await;
    let conn = stash.connection();

    conn.transaction().await.unwrap();
    let mut stored = StoredAction::new::<TestAction>(&state, Metadata::default()).unwrap();
    stored.save_using(&conn).await.unwrap();
    let first_action_id = stored.id.unwrap();
    conn.commit().await.unwrap();

    let metadata = MetadataBuilder::new()
        .with_debug_string("my debug string")
        .with_dependency(first_action_id)
        .with_resource(&"Resource")
        .unwrap()
        .build();

    let mut stored = StoredAction::new::<TestAction>(&state, metadata.clone()).unwrap();

    conn.transaction().await.unwrap();
    let mut stored_clone = stored.clone();
    stored_clone.save_using(&conn).await.unwrap();
    let id = stored_clone.id.unwrap();
    conn.commit().await.unwrap();
    let db_action = StoredAction::load(id, &conn).await.unwrap().unwrap();

    stored.id = Some(id);
    assert_eq!(stored, db_action);

    // delete action should delete both actions
    conn.transaction().await.unwrap();
    StoredAction::delete(&conn, first_action_id).await.unwrap();
    conn.commit().await.unwrap();
    let remaining = StoredAction::pending_count(&conn).await.unwrap();
    assert_eq!(remaining, 1);
}

async fn new_test_connection() -> Stash {
    let stash = Stash::new(None).unwrap();
    create_tables(&stash).await.unwrap();
    stash
}
