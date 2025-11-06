use super::common::{DefaultError, new_factory};
use super::common::{new_queue_with_stash, new_stash};
use proton_action_queue::action;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, FactoryResult, Handler, Type, VersionConverter,
    WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

const STARTING_VALUE: u32 = 30;
const END_VALUE: &str = "foo=30";

#[tokio::test]
async fn queued_version_migration() {
    // Queue action with one version, then try to execute action as v2.
    let pool = new_stash().await;
    let factory_v1 = new_factory::<V1Action>(V1ActionHandler);
    let factory_v2 = new_factory::<V2Action>(V2ActionHandler);
    let queue = new_queue_with_stash(pool.clone(), factory_v1).await;

    let queued_id = queue
        .queue_action(V1Action {
            value: STARTING_VALUE,
        })
        .await
        .unwrap()
        .id;
    drop(queue);

    let queue = new_queue_with_stash(pool.clone(), factory_v2).await;
    assert!(queue.contains(queued_id).await.unwrap());
    let executor = queue.new_executor();
    executor.execute_all().await.unwrap();
}

#[derive(Serialize, Deserialize)]
struct V1Action {
    value: u32,
}

impl Action for V1Action {
    const TYPE: Type = Type("action");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = V1ActionHandler;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
struct V1ActionHandler;

impl Handler for V1ActionHandler {
    type Action = V1Action;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
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
struct V2Action {
    value: String,
}

impl Action for V2Action {
    const TYPE: Type = Type("action");
    const VERSION: u32 = 2;

    type VersionConverter = V2VersionConverter;
    type Handler = V2ActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
}

struct V2VersionConverter;

impl VersionConverter for V2VersionConverter {
    type Output = V2Action;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        assert_eq!(old_version, V1Action::VERSION);
        assert_eq!(current_version, V2Action::VERSION);

        let v1 = action::deserialize::<V1Action>(data)?;

        Ok(V2Action {
            value: format!("foo={}", v1.value),
        })
    }
}

#[derive(Default)]
struct V2ActionHandler;

impl Handler for V2ActionHandler {
    type Action = V2Action;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
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
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        assert_eq!(action.value, END_VALUE);
        Ok(())
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
