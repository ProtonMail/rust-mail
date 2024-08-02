mod common;

use crate::common::{DefaultError, TestExtension};
use common::{new_factory, new_queue, new_session};
use proton_action_queue::action::{Action, DefaultVersionConverter, Handler, Type};
use proton_action_queue::queue::ActionStatus;
use proton_api_core::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::{Stash, Tether};

#[tokio::test]
async fn state_preserved_after_local_change() {
    // Check if the action state is persisted after local changes and correctly transmitted
    // to subsequent follow ups.

    let session = new_session();
    let queue = new_queue(new_factory::<TestAction>()).await;

    // Check direct execution.
    let output = queue
        .apply_action(&session, TestAction { v: ACTION_VALUE })
        .await
        .unwrap();
    assert!(matches!(output, ActionStatus::Executed(ACTION_VALUE_FINAL)));

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
    queue.execute_all(&session).await.unwrap();
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
    type Output = u32;
    type Error = DefaultError;
}

#[derive(Default)]
struct TestActionHandler {}

const ACTION_VALUE: u32 = 10;
const ACTION_VALUE_AFTER_LOCAL_APPLY: u32 = 30;
const ACTION_VALUE_FINAL: u32 = 512;

const ACTION_KEY: &str = "bar";

impl Handler for TestActionHandler {
    type Action = TestAction;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE);
        action.v = ACTION_VALUE_AFTER_LOCAL_APPLY;
        Ok(tx
            .ext_insert_value(ACTION_KEY, ACTION_VALUE_AFTER_LOCAL_APPLY)
            .await?)
    }

    async fn revert_local(
        &self,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        _: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        assert_eq!(action.v, ACTION_VALUE_AFTER_LOCAL_APPLY);

        stash
            .connection()
            .ext_insert_value(ACTION_KEY, ACTION_VALUE_FINAL)
            .await?;

        Ok(ACTION_VALUE_FINAL)
    }
}
