#![allow(non_snake_case)]

use super::*;
use crate::action::{
    Action, DefaultVersionConverter, Factory, MetadataBuilder, NoopError, Priority, Type,
};
use crate::tests::common::NoopActionHandler;
use serde::{Deserialize, Serialize};
use stash::stash::{Stash, StashConfiguration};
use std::time::Duration;

#[derive(Copy, Clone, Serialize, Deserialize)]
struct TestAction {
    v: u32,
}
impl Action for TestAction {
    const TYPE: Type = Type("test_action");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = NoopActionHandler<Self>;

    type RemoteOutput = ();
    type LocalOutput = ();

    type Error = NoopError;
    type Context = ();
}

#[tokio::test]
async fn check_action_priority() {
    // Check that an actions are popped from the queue ordered by priority and time.
    let queue = new_queue().await;
    let action = TestAction { v: 10 };

    let id0 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::Normal)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id1 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::Low)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id2 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::Highest)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id3 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::High)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id4 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::Highest)
                .build(),
        )
        .await
        .unwrap()
        .id;

    // Expected order:
    // * 2 Highest, oldest
    // * 4 Highest, more recent
    // * 3 High,
    // * 0 Normal,
    // * 1 Low

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id2));
    queue.delete_action(id2).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id4));
    queue.delete_action(id4).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id3));
    queue.delete_action(id3).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id0));
    queue.delete_action(id0).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id1));
    queue.delete_action(id1).await.unwrap();

    let next_action = queue.next_action().await.unwrap();
    assert!(next_action.is_none());
}

#[tokio::test]
async fn check_action_delay() {
    // Check that an actions are popped from the queue ordered by priority and delay time.
    let queue = new_queue().await;
    let action = TestAction { v: 10 };

    let date_time = chrono::Utc::now();

    let id0 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_creation_time(date_time)
                .with_delay(Duration::from_secs(1))
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id1 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new().with_creation_time(date_time).build(),
        )
        .await
        .unwrap()
        .id;

    let id2 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_creation_time(date_time)
                .with_delay(Duration::from_secs(1))
                .with_priority_override(Priority::Highest)
                .build(),
        )
        .await
        .unwrap()
        .id;

    // Expected order:
    // * 1 No delay
    // * 2 Highest (delay 1s)
    // * 0 Normal (delay 1s)

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id1));
    queue.delete_action(id1).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id2));
    queue.delete_action(id2).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id0));
    queue.delete_action(id0).await.unwrap();

    let next_action = queue.next_action().await.unwrap();
    assert!(next_action.is_none());
}

#[tokio::test]
async fn check_action_only_executed_without_dependencies() {
    let queue = new_queue().await;
    let action = TestAction { v: 10 };

    let id0 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_priority_override(Priority::Low)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id1 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_dependency(id0)
                .with_priority_override(Priority::Normal)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id2 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_dependency(id0)
                .with_dependency(id1)
                .with_priority_override(Priority::Normal)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id3 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_dependency(id0)
                .with_dependency(id1)
                .with_priority_override(Priority::High)
                .build(),
        )
        .await
        .unwrap()
        .id;

    let id4 = queue
        .queue_action_with_metadata(
            action,
            MetadataBuilder::new()
                .with_dependency(id0)
                .with_dependency(id2)
                .with_priority_override(Priority::Highest)
                .build(),
        )
        .await
        .unwrap()
        .id;

    // Expected order
    // * 0 - No Deps
    // * 1 - Depends on 0
    // * 3 - Depends on 0 & 1 (High)
    // * 2 - Depends on 0 & 1 (Normal)
    // * 4 - Depends on 2 & 0 (Highest)

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id0));
    queue.delete_action(id0).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id1));
    queue.delete_action(id1).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id3));
    queue.delete_action(id3).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id2));
    queue.delete_action(id2).await.unwrap();

    let next_action = queue.next_action().await.unwrap().unwrap();
    assert_eq!(next_action.id, Some(id4));
    queue.delete_action(id4).await.unwrap();

    let next_action = queue.next_action().await.unwrap();
    assert!(next_action.is_none());
}

async fn new_queue() -> Queue {
    let mut factory = Factory::new();
    factory.register::<TestAction>().unwrap();
    let pool = Stash::new(StashConfiguration::test()).unwrap();
    Queue::with_factory(pool, factory).await.unwrap()
}
