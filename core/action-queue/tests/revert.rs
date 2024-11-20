mod common;

use crate::common::DefaultError;
use common::{new_queue_typed, TestExtension};
use proton_action_queue::action::{Action, DefaultVersionConverter, Handler, Type};
use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use serde::{Deserialize, Serialize};
use stash::stash::{Stash, Tether};

#[tokio::test]
async fn network_failure_causes_revert_on_apply() {
    // Check that if remote fails to execute when action is applied, local state is reverted.
    let queue = new_queue_typed::<RevertAction>().await;

    let key = "foo";
    let value = 30_u32;
    let result = queue
        .apply_action(RevertAction {
            key: key.to_string(),
            value,
        })
        .await;
    assert!(matches!(
        result,
        Err(ActionError::<RevertAction>::Action(
            DefaultError::APIFailure
        ))
    ));
    assert!(queue
        .stash()
        .connection()
        .ext_get_value(key)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn network_failure_causes_revert_on_queue() {
    // Check that if remote fails to execute when action is queued, local state is reverted.
    let queue = new_queue_typed::<RevertAction>().await;

    let key = "foo";
    let value = 30_u32;
    let action_id = queue
        .queue_action(RevertAction {
            key: key.to_string(),
            value,
        })
        .await
        .unwrap()
        .id;

    // Check local state is present.
    assert_eq!(
        queue
            .stash()
            .connection()
            .ext_get_value(key)
            .await
            .unwrap()
            .unwrap(),
        value
    );

    let QueuedError::Action(error, metadata) = queue.execute_all().await.unwrap_err() else {
        panic!("unexpected queued action error");
    };

    let down_casted = error.as_action_error::<RevertAction>().unwrap();
    assert!(matches!(
        down_casted,
        ActionError::<RevertAction>::Action(DefaultError::APIFailure)
    ));

    assert_eq!(metadata.id, action_id);
    assert!(queue
        .stash()
        .connection()
        .ext_get_value(key)
        .await
        .unwrap()
        .is_none());
}

#[derive(Serialize, Deserialize)]
struct RevertAction {
    key: String,
    value: u32,
}

impl Action for RevertAction {
    const TYPE: Type = Type("revert");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = RevertActionHandler;
    type RemoteOutput = u32;

    type LocalOutput = ();
    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
struct RevertActionHandler {}

impl Handler for RevertActionHandler {
    type Action = RevertAction;
    type Context = ();

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_insert_value(&action.key, action.value).await?)
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_delete_value(&action.key).await?)
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Err(DefaultError::APIFailure)
    }
}
