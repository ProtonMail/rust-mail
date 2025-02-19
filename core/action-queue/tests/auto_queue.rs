#![allow(clippy::ignored_unit_patterns)]
mod common;

use crate::common::DefaultError;
use common::new_queue_typed;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard, WriterGuardError,
};
use proton_action_queue::queue::QueuedActionState;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[tokio::test]
async fn auto_queued_on_network_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let queue = new_queue_typed::<ErrorAction>().await;

    queue.queue_action(ErrorAction {}).await.unwrap();
    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(output, QueuedActionState::Queued(_)),);
}

#[tokio::test]
async fn auto_queued_on_writer_guard_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let queue = new_queue_typed::<WriteGuardExpiredAction>().await;

    queue
        .queue_action(WriteGuardExpiredAction {})
        .await
        .unwrap();
    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(output, QueuedActionState::Queued(_)),);
}

#[tokio::test]
async fn execute_all_does_not_loop_forever_on_network_failure() {
    // There was a bug where execute all would loop forever if an action was re-queued due to
    // network failure.
    let queue = new_queue_typed::<ErrorAction>().await;

    let _ = queue.queue_action(ErrorAction {}).await.unwrap();

    queue.new_executor().execute_all().await.unwrap();
}

#[derive(Serialize, Deserialize)]
struct ErrorAction {}

impl Action for ErrorAction {
    const TYPE: Type = Type("error");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ErrorActionHandler;
    type RemoteOutput = u32;

    type LocalOutput = ();

    type Error = DefaultError;

    type Context = ();
}

#[derive(Default)]
struct ErrorActionHandler {}

impl Handler for ErrorActionHandler {
    type Action = ErrorAction;

    type Context = ();

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
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
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Err(DefaultError::NetworkFailure)
    }
}

#[derive(Serialize, Deserialize)]
struct WriteGuardExpiredAction {}
impl Action for WriteGuardExpiredAction {
    const TYPE: Type = Type("writer_guard_expired");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = WriterGuardExpiredActionHandler;
    type RemoteOutput = u32;

    type LocalOutput = ();

    type Error = DefaultError;

    type Context = ();
}

#[derive(Default)]
struct WriterGuardExpiredActionHandler {}

impl Handler for WriterGuardExpiredActionHandler {
    type Action = WriteGuardExpiredAction;

    type Context = ();

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
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
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Err(WriterGuardError::Expired.into())
    }
}
