use super::common::{DefaultError, TestWriteExtension};
use super::common::{new_factory, new_queue};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, MetadataBuilder, Type, WriterGuard,
};
use proton_action_queue::db::ExecutionGuard;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn replace_updates_local_state() {
    // When replacing, check that local state is updated when the action is stored.

    let queue = new_queue(new_factory::<TestAction>()).await;
    let executor = queue.new_executor();

    // Check direct execution.
    let queued_output = queue
        .queue_action(TestAction {
            v: ACTION_VALUE_AFTER_LOCAL_APPLY,
        })
        .await
        .unwrap();

    // queue replacement

    let replaced_output = queue
        .replace_or_queue_action(
            queued_output.id,
            TestAction {
                v: ACTION_VALUE_AFTER_REPLACE,
            },
        )
        .await
        .unwrap();
    assert_eq!(replaced_output.id, queued_output.id);

    // Execute the action.
    let executed = executor.execute_all().await.unwrap();
    assert_eq!(executed, 1);
}

#[tokio::test]
async fn replace_updates_queues_if_action_no_longer_present() {
    // When attempting to replace an action that does not exist, it will be
    // queued instead.
    let queue = new_queue(new_factory::<TestAction>()).await;
    let executor = queue.new_executor();

    // Check direct execution.
    let queued_output = queue
        .queue_action(TestAction {
            v: ACTION_VALUE_AFTER_LOCAL_APPLY,
        })
        .await
        .unwrap();

    // remove action.
    queue.cancel(queued_output.id).await.unwrap();

    // queue replacement
    let replaced_output = queue
        .replace_or_queue_action(
            queued_output.id,
            TestAction {
                v: ACTION_VALUE_AFTER_REPLACE,
            },
        )
        .await
        .unwrap();
    assert_ne!(replaced_output.id, queued_output.id);

    // Execute the action.
    let executed = executor.execute_all().await.unwrap();
    assert_eq!(executed, 1);
}

#[tokio::test]
async fn replace_updates_queues_if_action_is_executing() {
    // When attempting to replace an action that does not exist, it will be
    // queued instead.
    let queue = new_queue(new_factory::<TestAction>()).await;

    // Check direct execution.
    let queued_output = queue
        .queue_action(TestAction {
            v: ACTION_VALUE_AFTER_LOCAL_APPLY,
        })
        .await
        .unwrap();

    // simulate action executing
    let mut tether = queue.stash().connection();
    tether
        .tx(async |tx| ExecutionGuard::acquire(queued_output.id, "TEST", tx).await)
        .await
        .unwrap();

    // queue replacement
    let replaced_output = queue
        .replace_or_queue_action(
            queued_output.id,
            TestAction {
                v: ACTION_VALUE_AFTER_REPLACE,
            },
        )
        .await
        .unwrap();
    assert_ne!(replaced_output.id, queued_output.id);
}

#[tokio::test]
async fn replace_updates_local_state_with_resources() {
    // There was a subtle failure related to the resource table having duplicate entries.
    // This test makes sure this doesn't happen.

    let queue = new_queue(new_factory::<TestAction>()).await;
    let mut counter: usize = 10;

    let metadata = MetadataBuilder::default()
        .with_resource(&counter)
        .unwrap()
        .build();

    // Check direct execution.
    let queued_output = queue
        .queue_action_with_metadata(
            TestAction {
                v: ACTION_VALUE_AFTER_LOCAL_APPLY,
            },
            metadata,
        )
        .await
        .unwrap();

    // queue replacement
    for _ in 0..10 {
        counter += 1;
        let metadata = MetadataBuilder::default()
            .with_resource(&counter)
            .unwrap()
            .build();
        let replaced_output = queue
            .replace_or_queue_action_with_metadata(
                queued_output.id,
                TestAction {
                    v: ACTION_VALUE_AFTER_REPLACE,
                },
                metadata,
            )
            .await
            .unwrap();
        assert_eq!(replaced_output.id, queued_output.id);
    }

    // Execute the action.
    let executor = queue.new_executor();
    let executed = executor.execute_all().await.unwrap();
    assert_eq!(executed, 1);
}

#[derive(Serialize, Deserialize)]
struct TestAction {
    v: u32,
}

impl Action for TestAction {
    const TYPE: Type = Type("test");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = TestActionHandler;
    type RemoteOutput = u32;

    type LocalOutput = ();

    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
struct TestActionHandler {}

const ACTION_VALUE_AFTER_LOCAL_APPLY: u32 = 10;
const ACTION_VALUE_AFTER_REPLACE: u32 = 30;
const ACTION_KEY: &str = "bar";

impl Handler for TestActionHandler {
    type Action = TestAction;
    type Context = ();

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_insert_value(ACTION_KEY, action.v).await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // do nothing
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE_AFTER_REPLACE);
        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                Ok(tx
                    .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_REPLACE)
                    .await?)
            })
            .await?;

        Ok(ACTION_VALUE_AFTER_REPLACE)
    }
}
