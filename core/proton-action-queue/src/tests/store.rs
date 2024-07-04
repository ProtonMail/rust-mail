#![allow(non_snake_case)]

use super::*;
use crate::define_action_id;
use serde::{Deserialize, Serialize};

impl PendingAction {
    pub(crate) fn from_action_and_priority<T: Action>(
        action: &T,
        action_priority: ActionPriority,
    ) -> Result<Self, rmp_serde::encode::Error> {
        let data = rmp_serde::to_vec(action)?;

        Ok(Self {
            action_id: action.action_id().clone(),
            version: action.action_version(),
            priority: action_priority,
            data,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TestAction {
    pub value: u32,
}

const TEST_ACTION_VERSION: u32 = 10;
define_action_id!(TEST_ACTION_ID, "b07e7108-6bbc-4426-9b03-67d23726bbac");
impl Action for TestAction {
    const ID: ActionId = TEST_ACTION_ID;
    const VERSION: u32 = TEST_ACTION_VERSION;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TestAction2 {
    pub value: String,
}

const TEST_ACTION2_VERSION: u32 = 99;
define_action_id!(TEST_ACTION2_ID, "3e257729-7f27-42d5-b127-0d28731a69c1");
impl Action for TestAction2 {
    const ID: ActionId = TEST_ACTION2_ID;
    const VERSION: u32 = TEST_ACTION2_VERSION;
}

#[tokio::test]
async fn action_insert_and_retrieval() {
    let action1 = TestAction { value: 0 };

    let action2 = TestAction2 {
        value: "hello_world!".into(),
    };

    let queue = new_queue().await;
    let tx = queue
        .stash
        .transaction()
        .await
        .expect("failed to start transaction");
    let mut store = ActionStore::new(tx.clone());
    let pending1 = PendingAction::from_action(&action1).expect("failed to create pending action");
    let pending2 = PendingAction::from_action(&action2).expect("failed to create pending action");
    let stored_ids = store
        .store_actions(&[pending1, pending2])
        .await
        .expect("failed to store action");

    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[0]);
        assert_eq!(stored_action.action_id, *action1.action_id());
        assert_eq!(stored_action.version, action1.action_version());
        let deserialized = stored_action
            .deserialize::<TestAction>()
            .expect("failed to deserialize");
        assert_eq!(deserialized, action1);
        store
            .erase_actions(&[stored_ids[0]])
            .await
            .expect("failed to remove stored action");
    }

    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[1]);
        assert_eq!(stored_action.action_id, *action2.action_id());
        assert_eq!(stored_action.version, action2.action_version());
        let deserialized = stored_action
            .deserialize::<TestAction2>()
            .expect("failed to deserialize");
        assert_eq!(deserialized, action2);
        store
            .erase_actions(&[stored_ids[1]])
            .await
            .expect("failed to remove stored action");
    }
    tx.commit().await.expect("transaction failed");
}

#[tokio::test]
async fn action_insert_and_retrieval_with_priority() {
    let action1 = TestAction { value: 0 };

    let action2 = TestAction2 {
        value: "hello_world!".into(),
    };

    let action3 = TestAction { value: 0 };

    let action4 = TestAction { value: 0 };

    let queue = new_queue().await;
    let tx = queue
        .stash
        .transaction()
        .await
        .expect("failed to start transaction");
    let mut store = ActionStore::new(tx.clone());
    let pending1 = PendingAction::from_action_and_priority(&action1, ActionPriority::Low)
        .expect("failed to create pending action");
    let pending2 = PendingAction::from_action_and_priority(&action2, ActionPriority::Highest)
        .expect("failed to create pending action");
    let pending3 = PendingAction::from_action_and_priority(&action3, ActionPriority::Normal)
        .expect("failed to create pending action");
    let pending4 = PendingAction::from_action_and_priority(&action4, ActionPriority::Low)
        .expect("failed to create pending action");
    let stored_ids = store
        .store_actions(&[pending1, pending2, pending3, pending4])
        .await
        .expect("failed to store action");

    // Actions should be consumed in the following index order: 1,2,0,3
    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[1]);
        store
            .erase_actions(&[stored_ids[1]])
            .await
            .expect("failed to remove stored action");
    }

    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[2]);
        store
            .erase_actions(&[stored_ids[2]])
            .await
            .expect("failed to remove stored action");
    }

    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[0]);
        store
            .erase_actions(&[stored_ids[0]])
            .await
            .expect("failed to remove stored action");
    }
    {
        let stored_action = store
            .get_next_action()
            .await
            .expect("failed to get next action")
            .expect("action must be present");
        assert_eq!(stored_action.id, stored_ids[3]);
        store
            .erase_actions(&[stored_ids[3]])
            .await
            .expect("failed to remove stored action");
    }
    tx.commit().await.expect("transaction failed");
}

async fn new_queue() -> crate::ActionQueue {
    let stash = Stash::new(None).expect("Failed to create Stash");
    ActionStore::init_tables(&stash)
        .await
        .expect("failed to init store tables");
    let factory = crate::ActionFactory::new();

    crate::ActionQueue::new(
        stash,
        Box::new(crate::AlwaysErrorSessionProvider {}),
        factory,
    )
}
