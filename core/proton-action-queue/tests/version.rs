mod common;

use crate::common::{new_factory, DefaultError};
use common::{new_queue_with_stash, new_session, new_stash};
use proton_action_queue::action;
use proton_action_queue::action::{
    Action, DefaultVersionConverter, FactoryResult, Handler, Type, VersionConverter,
};
use proton_api_core::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::Tether;

const STARTING_VALUE: u32 = 30;
const END_VALUE: &str = "foo=30";

#[tokio::test]
async fn queued_version_migration() {
    // Queue action with one version, then try to execute action as v2.
    let pool = new_stash().await;
    let factory_v1 = new_factory::<V1Action>();
    let factory_v2 = new_factory::<V2Action>();
    let queue = new_queue_with_stash(pool.clone(), factory_v1).await;

    let queued_id = queue
        .queue_action(V1Action {
            value: STARTING_VALUE,
        })
        .await
        .unwrap();
    drop(queue);

    let queue = new_queue_with_stash(pool.clone(), factory_v2).await;
    assert!(queue.contains(queued_id).await.unwrap());
    let session = new_session();
    queue.execute_all(&session).await.unwrap()
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
    type Output = u32;
    type Error = DefaultError;
}

#[derive(Default)]
struct V1ActionHandler {}

impl Handler for V1ActionHandler {
    type Action = V1Action;

    async fn apply_local(
        &self,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
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
        _: &mut Self::Action,
        _: &Session,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
    }

    async fn apply_local_post_remote(
        &self,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        panic!("should not be called");
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
    type Output = ();
    type Error = DefaultError;
}

struct V2VersionConverter {}
impl VersionConverter for V2VersionConverter {
    type Output = V2Action;
    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        assert_eq!(old_version, V1Action::VERSION);
        assert_eq!(current_version, V2Action::VERSION);

        let v1 = action::deserialize::<V1Action>(&data)?;

        Ok(V2Action {
            value: format!("foo={}", v1.value),
        })
    }
}
#[derive(Default)]
struct V2ActionHandler {}

impl Handler for V2ActionHandler {
    type Action = V2Action;

    async fn apply_local(
        &self,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
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
    ) -> Result<(), <Self::Action as Action>::Error> {
        assert_eq!(action.value, END_VALUE);
        Ok(())
    }

    async fn apply_local_post_remote(
        &self,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        Ok(())
    }
}
