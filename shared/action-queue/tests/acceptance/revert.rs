use super::common::DefaultError;
use super::common::{TestReadExtension, TestWriteExtension, new_queue_typed};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, MetadataBuilder, Type, WriterGuard,
};
use proton_action_queue::queue::{ActionError, AsActionError, BroadcastMessage, QueuedError};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn network_failure_causes_revert_on_apply() {
    // Check that if remote fails to execute when action is applied, local state is reverted.
    let queue = new_queue_typed::<RevertAction>(RevertActionHandler).await;

    let key = "foo";
    let value = 30_u32;
    queue
        .queue_action(RevertAction {
            key: key.to_string(),
            value,
        })
        .await
        .unwrap();
    let result = queue.new_executor().execute_one().await.unwrap_err();
    match result {
        QueuedError::Action(e, _) => {
            let err = e.as_action_error::<RevertAction>().unwrap();
            assert!(matches!(
                err,
                ActionError::<RevertAction>::Action(DefaultError::APIFailure)
            ));
        }
        _ => panic!("unexpected result"),
    }
    assert!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(key)
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn network_failure_causes_revert_on_queue() {
    // Check that if remote fails to execute when action is queued, local state is reverted.
    let queue = new_queue_typed::<RevertAction>(RevertActionHandler).await;
    let executor = queue.new_executor();

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
            .await
            .unwrap()
            .ext_get_value(key)
            .await
            .unwrap()
            .unwrap(),
        value
    );

    let QueuedError::Action(error, metadata) = executor.execute_all().await.unwrap_err() else {
        panic!("unexpected queued action error");
    };

    let down_casted = error.as_action_error::<RevertAction>().unwrap();
    assert!(matches!(
        down_casted,
        ActionError::<RevertAction>::Action(DefaultError::APIFailure)
    ));

    assert_eq!(metadata.id, action_id);
    assert!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(key)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn revert_cancels_all_dependent_actions() {
    // Check that if an action fails to execute and all the subsequent actions
    // that depend on the failed action also revert.
    let queue = new_queue_typed::<ChainCancelAction>(ChainCancelActionHandler).await;
    let executor = queue.new_executor();

    let key = "foo";
    let value = 30_u32;
    let value2 = 1245_u32;
    let value3 = 100_u32;
    let value4 = 400_u32;

    {
        let mut conn = queue.stash().connection().await.unwrap();
        conn.tx(async |tx| tx.ext_insert_value(key, value).await)
            .await
            .unwrap();
    }

    let action_id1 = queue
        .queue_action(ChainCancelAction {
            key: key.to_string(),
            value: value2,
            old_value: 0,
        })
        .await
        .unwrap()
        .id;

    let action_id2 = queue
        .queue_action_with_metadata(
            ChainCancelAction {
                key: key.to_string(),
                value: value3,
                old_value: 0,
            },
            MetadataBuilder::new().with_dependency(action_id1).build(),
        )
        .await
        .unwrap()
        .id;

    let action_id3 = queue
        .queue_action_with_metadata(
            ChainCancelAction {
                key: key.to_string(),
                value: value4,
                old_value: 0,
            },
            MetadataBuilder::new()
                .with_dependencies([action_id1, action_id2])
                .build(),
        )
        .await
        .unwrap()
        .id;

    // Check local state is present.
    assert_eq!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(key)
            .await
            .unwrap()
            .unwrap(),
        value4
    );

    let mut broadcast = queue.new_broadcast_receiver();

    // Cancel
    executor.execute_all().await.expect_err("Should fail");

    let output = broadcast.recv().await.unwrap();
    assert!(matches!(output, BroadcastMessage::Cancelled(_)));

    // Check state is reverted.
    assert_eq!(
        queue
            .stash()
            .connection()
            .await
            .unwrap()
            .ext_get_value(key)
            .await
            .unwrap()
            .unwrap(),
        value
    );

    assert!(!queue.contains(action_id1).await.unwrap());
    assert!(!queue.contains(action_id3).await.unwrap());
    assert!(!queue.contains(action_id2).await.unwrap());
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
}

#[derive(Default)]
struct RevertActionHandler;

impl Handler for RevertActionHandler {
    type Action = RevertAction;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_insert_value(&action.key, action.value).await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_delete_value(&action.key).await?)
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Err(DefaultError::APIFailure)
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct ChainCancelAction {
    pub key: String,
    pub value: u32,
    old_value: u32,
}

impl Action for ChainCancelAction {
    const TYPE: Type = Type("chain_revert");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ChainCancelActionHandler;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
pub struct ChainCancelActionHandler;

impl Handler for ChainCancelActionHandler {
    type Action = ChainCancelAction;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let old_value = tx.ext_get_value(&action.key).await?.unwrap();
        action.old_value = old_value;
        Ok(tx.ext_insert_value(&action.key, action.value).await?)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let current_value = tx.ext_get_value(&action.key).await?.unwrap();
        assert_eq!(current_value, action.value);
        Ok(tx.ext_insert_value(&action.key, action.old_value).await?)
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Err(DefaultError::APIFailure)
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
