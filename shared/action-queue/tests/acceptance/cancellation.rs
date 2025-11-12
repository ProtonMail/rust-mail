use super::common::DefaultError;
use super::common::{TestReadExtension, TestWriteExtension, new_queue_typed};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, MetadataBuilder, Type, WriterGuard,
};
use proton_action_queue::queue::{ActionError, Error, QueuedError};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn cancel_causes_revert() {
    // Check that cancellation reverts local state.
    let queue = new_queue_typed::<CancelAction>(CancelActionHandler).await;

    let key = "foo";
    let value = 30_u32;
    let action_id = queue
        .queue_action(CancelAction {
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

    // Cancel
    queue.cancel(action_id).await.unwrap();

    // Check state is reverted.
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
    let queue = new_queue_typed::<ChainCancelAction>(ChainCancelActionHandler).await;

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

    // Cancel
    let cancelled = queue.cancel(action_id1).await.unwrap();
    assert!(cancelled.contains(&action_id2));
    assert!(cancelled.contains(&action_id3));

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

#[tokio::test]
async fn accidental_cyclic_dependency_with_replace() {
    fn create_action(value: u32) -> ChainCancelAction {
        ChainCancelAction {
            key: "foo".to_string(),
            value,
            old_value: 0,
        }
    }

    // Action 143 has [ActionId(145), ActionId(146), ActionId(147), ActionId(148)]
    // 147:[143]
    // 184:[]
    let queue = new_queue_typed::<ChainCancelAction>(ChainCancelActionHandler).await;

    let action_143 = create_action(143);
    let action_145 = create_action(145);
    let action_146 = create_action(146);
    let action_147 = create_action(147);
    let action_148 = create_action(148);

    {
        let mut conn = queue.stash().connection().await.unwrap();
        conn.tx(async |tx| tx.ext_insert_value("foo", 0).await)
            .await
            .unwrap();
    }

    let action_148_id = queue.queue_action(action_148).await.unwrap().id;
    let action_145_id = queue.queue_action(action_145).await.unwrap().id;
    let action_146_id = queue.queue_action(action_146).await.unwrap().id;
    let action_147_id = queue.queue_action(action_147).await.unwrap().id;
    let action_143_id = queue
        .queue_action_with_metadata(
            action_143,
            MetadataBuilder::new()
                .with_dependencies([action_146_id, action_147_id, action_145_id, action_148_id])
                .build(),
        )
        .await
        .unwrap()
        .id;

    let Err(err) = queue
        .replace_or_queue_action_with_metadata(
            action_147_id,
            create_action(1472),
            MetadataBuilder::new()
                .with_dependency(action_143_id)
                .build(),
        )
        .await
    else {
        panic!("expected error");
    };
    assert!(matches!(err, ActionError::Queue(Error::CyclicDependency)));
}

#[tokio::test]
async fn cancel_causes_revert_to_only_direct_dependees() {
    // Check that cancellation reverts local state and all the subsequent actions
    // that depend on the cancelled action.
    let queue = new_queue_typed::<ChainCancelAction>(ChainCancelActionHandler).await;

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
                .with_optional_dependencies([action_id2])
                .build(),
        )
        .await
        .unwrap()
        .id;

    // Cancel first action and observe the last action is still present.
    let cancelled = queue.cancel(action_id1).await.unwrap();
    assert!(cancelled.contains(&action_id2));
    assert!(!cancelled.contains(&action_id3));
    assert!(!queue.contains(action_id1).await.unwrap());
    assert!(queue.contains(action_id3).await.unwrap());
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
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
pub struct CancelActionHandler;

impl Handler for CancelActionHandler {
    type Action = CancelAction;

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
        panic!("should not be called");
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
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
        Ok(tx.ext_insert_value(&action.key, action.old_value).await?)
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        panic!("should not be called");
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
