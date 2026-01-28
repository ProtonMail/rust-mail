use super::common::{DefaultError, TestWriteExtension};
use super::common::{new_factory, new_queue};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, MetadataBuilder, Type, WriterGuard,
};
use proton_action_queue::db::{DependencyType, ExecutionGuard, StoredAction};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_action_queue::tests::common::TestDb;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn replace_updates_local_state() {
    // When replacing, check that local state is updated when the action is stored.

    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;
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
    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;
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
    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;

    // Check direct execution.
    let queued_output = queue
        .queue_action(TestAction {
            v: ACTION_VALUE_AFTER_LOCAL_APPLY,
        })
        .await
        .unwrap();

    // simulate action executing
    let mut tether = queue.stash().connection().await.unwrap();
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

    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;
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

#[tokio::test]
async fn replace_updates_previous_dependencies_type() {
    // Action queued with optional dependency, should have that dependency replaced with the
    // required type if it is overwritten.
    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;
    let queued_output_dep = queue
        .queue_action(TestAction {
            v: ACTION_VALUE_AFTER_LOCAL_APPLY,
        })
        .await
        .unwrap();

    let metadata = MetadataBuilder::default()
        .with_optional_dependency(queued_output_dep.id)
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
    let metadata = MetadataBuilder::default()
        .with_dependency(queued_output_dep.id)
        .build();

    // Check direct execution.
    let queued_output2 = queue
        .replace_or_queue_action_with_metadata(
            queued_output.id,
            TestAction {
                v: ACTION_VALUE_AFTER_LOCAL_APPLY,
            },
            metadata,
        )
        .await
        .unwrap();

    assert_eq!(queued_output2.id, queued_output.id);
    let deps = StoredAction::all_dependencies(&queue.tether().await.unwrap(), queued_output2.id)
        .await
        .unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].dependency_id, queued_output_dep.id);
    assert_eq!(deps[0].dependency_type, DependencyType::Required);
}

#[derive(Serialize, Deserialize)]
struct TestAction {
    v: u32,
}

impl Action<TestDb> for TestAction {
    const TYPE: Type = Type("test");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = TestActionHandler;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
struct TestActionHandler;

const ACTION_VALUE_AFTER_LOCAL_APPLY: u32 = 10;
const ACTION_VALUE_AFTER_REPLACE: u32 = 30;
const ACTION_KEY: &str = "bar";

impl Handler<TestDb> for TestActionHandler {
    type Action = TestAction;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_, TestDb>,
    ) -> Result<(), <Self::Action as Action<TestDb>>::Error> {
        Ok(tx.ext_insert_value(ACTION_KEY, action.v).await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_, TestDb>,
    ) -> Result<(), <Self::Action as Action<TestDb>>::Error> {
        // do nothing
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_, TestDb>,
    ) -> Result<
        <Self::Action as Action<TestDb>>::RemoteOutput,
        <Self::Action as Action<TestDb>>::Error,
    > {
        assert_eq!(action.v, ACTION_VALUE_AFTER_REPLACE);
        guard
            .tx::<_, _, <Self::Action as Action<TestDb>>::Error>(async |tx| {
                Ok(tx
                    .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_REPLACE)
                    .await?)
            })
            .await?;

        Ok(ACTION_VALUE_AFTER_REPLACE)
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_, TestDb>,
    ) -> Result<(), <Self::Action as Action<TestDb>>::Error> {
        Ok(())
    }
}
