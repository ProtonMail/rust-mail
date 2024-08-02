mod common;

use crate::common::DefaultError;
use common::{new_queue_typed, TestExtension};
use proton_action_queue::action::{
    Action, DefaultVersionConverter, Handler, MetadataBuilder, Type,
};
use proton_action_queue::queue::QueuedError;
use proton_api_core::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::{Stash, Tether};

#[tokio::test]
async fn cancel_causes_revert() {
    // Check that cancellation reverts local state.
    let queue = new_queue_typed::<CancelAction>().await;

    let key = "foo";
    let value = 30_u32;
    let action_id = queue
        .queue_action(CancelAction {
            key: key.to_string(),
            value,
        })
        .await
        .unwrap();

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

    // Cancel
    queue.cancel(action_id).await.unwrap();

    // Check state is reverted.
    assert!(queue
        .stash()
        .connection()
        .ext_get_value(key)
        .await
        .unwrap()
        .is_none());
    // Double cancel is error:
    assert!(matches!(
        queue.cancel(action_id).await.unwrap_err(),
        QueuedError::ActionNotFound(_)
    ));
}

#[tokio::test]
async fn cancel_causes_revert_with_dependees() {
    // Check that cancellation reverts local state and all the subsequent actions
    // that depend on the cancelled action.
    let queue = new_queue_typed::<ChainCancelAction>().await;

    let key = "foo";
    let value = 30_u32;
    let value2 = 1245_u32;
    let value3 = 100_u32;
    let value4 = 400_u32;

    {
        let tx = queue.stash().transaction().await.unwrap();
        tx.ext_insert_value(key, value).await.unwrap();
        tx.commit().await.unwrap();
    }

    let action_id1 = queue
        .queue_action(ChainCancelAction {
            key: key.to_string(),
            value: value2,
            old_value: 0,
        })
        .await
        .unwrap();

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
        .unwrap();

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
        .unwrap();

    // Check local state is present.
    assert_eq!(
        queue
            .stash()
            .connection()
            .ext_get_value(key)
            .await
            .unwrap()
            .unwrap(),
        value4
    );

    // Cancel
    let cancelled = queue.cancel_with_dependees(action_id1).await.unwrap();
    assert!(cancelled.contains(&action_id2));
    assert!(cancelled.contains(&action_id3));

    // Check state is reverted.
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

    assert!(!queue.contains(action_id1).await.unwrap());
    assert!(!queue.contains(action_id3).await.unwrap());
    assert!(!queue.contains(action_id2).await.unwrap());
}
#[derive(Serialize, Deserialize)]
pub struct CancelAction {
    pub key: String,
    pub value: u32,
}

impl Action for CancelAction {
    const TYPE: Type = Type("revert");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = CancelActionHandler;
    type Output = u32;
    type Error = DefaultError;
}

#[derive(Default)]
pub struct CancelActionHandler {}

impl Handler for CancelActionHandler {
    type Action = CancelAction;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_insert_value(&action.key, action.value).await?)
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_delete_value(&action.key).await?)
    }

    async fn apply_remote(
        &self,
        _: &mut Self::Action,
        _: &Session,
        _: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        panic!("should not be called");
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
    type Output = u32;
    type Error = DefaultError;
}

#[derive(Default)]
pub struct ChainCancelActionHandler {}

impl Handler for ChainCancelActionHandler {
    type Action = ChainCancelAction;
    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let old_value = tx.ext_get_value(&action.key).await?.unwrap();
        action.old_value = old_value;
        Ok(tx.ext_insert_value(&action.key, action.value).await?)
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(tx.ext_insert_value(&action.key, action.old_value).await?)
    }

    async fn apply_remote(
        &self,
        _: &mut Self::Action,
        _: &Session,
        _: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        panic!("should not be called");
    }
}
