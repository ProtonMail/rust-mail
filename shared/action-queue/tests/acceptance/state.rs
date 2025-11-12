use super::common::{DefaultError, TestReadExtension, TestWriteExtension};
use super::common::{new_factory, new_queue};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn state_preserved_after_local_change() {
    // Check if the action state is persisted after local changes and correctly transmitted
    // to subsequent follow ups.

    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;
    let executor = queue.new_executor();

    // Check direct execution.
    queue
        .queue_action(TestAction { v: ACTION_VALUE })
        .await
        .unwrap();

    executor.execute_one().await.unwrap();

    // Check local state is as expected.
    assert_eq!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(ACTION_KEY)
            .await
            .unwrap()
            .unwrap(),
        ACTION_VALUE_FINAL
    );

    // Check queue execution.
    queue
        .queue_action(TestAction { v: ACTION_VALUE })
        .await
        .unwrap();

    executor.execute_all().await.unwrap();
}

#[cfg(feature = "rebase")]
#[tokio::test]
async fn rebase_state() {
    // Check if the action state is persisted after local changes and correctly transmitted
    // to subsequent follow ups.

    let queue = new_queue(new_factory::<TestAction>(TestActionHandler)).await;

    // Check direct execution.
    queue
        .queue_action(TestAction { v: ACTION_VALUE })
        .await
        .unwrap();

    queue
        .stash()
        .connection()
        .await
        .unwrap()
        .tx(async |tx| tx.ext_insert_value(ACTION_KEY, 100).await)
        .await
        .unwrap();

    queue
        .rebase(TestAction::GROUP, &RebaseChangeSet::default())
        .await
        .unwrap();

    // Check local state is as expected.
    assert_eq!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(ACTION_KEY)
            .await
            .unwrap()
            .unwrap(),
        ACTION_VALUE_AFTER_LOCAL_APPLY
    );
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
}

#[derive(Default)]
struct TestActionHandler;

const ACTION_VALUE: u32 = 10;
const ACTION_VALUE_AFTER_LOCAL_APPLY: u32 = 30;
const ACTION_VALUE_FINAL: u32 = 512;

const ACTION_KEY: &str = "bar";

impl Handler for TestActionHandler {
    type Action = TestAction;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE);
        action.v = ACTION_VALUE_AFTER_LOCAL_APPLY;
        Ok(tx
            .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_LOCAL_APPLY)
            .await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE_AFTER_LOCAL_APPLY);
        writer_guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx: &Bond<'_>| {
                Ok(tx.ext_insert_value(ACTION_KEY, ACTION_VALUE_FINAL).await?)
            })
            .await?;

        Ok(ACTION_VALUE_FINAL)
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx
            .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_LOCAL_APPLY)
            .await?)
    }
}
