#![allow(clippy::ignored_unit_patterns)]
mod common;

use crate::common::{DefaultError, TestReadExtension, TestWriteExtension};
use common::{new_factory, new_queue};
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Handler, Type};
use proton_action_queue::queue::ActionRemoteOutput;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Stash};

#[tokio::test]
async fn state_preserved_after_local_change() {
    // Check if the action state is persisted after local changes and correctly transmitted
    // to subsequent follow ups.

    let queue = new_queue(new_factory::<TestAction>()).await;

    // Check direct execution.
    let output = queue
        .apply_action(TestAction { v: ACTION_VALUE })
        .await
        .unwrap();
    assert!(matches!(
        output.remote,
        ActionRemoteOutput::Executed(ACTION_VALUE_FINAL)
    ));

    // Check local state is as expected.
    assert_eq!(
        queue
            .stash()
            .connection()
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
    queue.execute_all().await.unwrap();
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

const ACTION_VALUE: u32 = 10;
const ACTION_VALUE_AFTER_LOCAL_APPLY: u32 = 30;
const ACTION_VALUE_FINAL: u32 = 512;

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
        assert_eq!(action.v, ACTION_VALUE);
        action.v = ACTION_VALUE_AFTER_LOCAL_APPLY;
        Ok(tx
            .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_LOCAL_APPLY)
            .await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE_AFTER_LOCAL_APPLY);
        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        tx.ext_insert_value(ACTION_KEY, ACTION_VALUE_FINAL).await?;
        tx.commit().await?;

        Ok(ACTION_VALUE_FINAL)
    }
}
