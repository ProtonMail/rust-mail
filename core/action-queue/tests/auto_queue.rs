#![allow(clippy::ignored_unit_patterns)]
mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::common::DefaultError;
use common::new_queue_typed;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard, WriterGuardError,
};
use proton_action_queue::network::{DummyWaitForOnline, WaitForOnline, WaitForOnlineSubscribtion};
use proton_action_queue::queue::{BroadcastMessage, QueuedActionReason, QueuedActionState};
use proton_task_service::TaskService;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tokio::time::sleep;

#[tokio::test]
async fn auto_queued_on_network_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let queue = new_queue_typed::<ErrorAction>().await;

    queue.queue_action(ErrorAction {}).await.unwrap();
    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(
        output,
        QueuedActionState::Queued(_, QueuedActionReason::Network)
    ),);
}

#[tokio::test]
async fn auto_queued_on_pause() {
    let queue = new_queue_typed::<SuccessAction>().await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_service = TaskService::new().unwrap();
    let auto_executor = queue
        .new_executor()
        .into_auto_executor(DummyWaitForOnline, &task_service);

    auto_executor.pause();
    queue.queue_action(SuccessAction {}).await.unwrap();

    sleep(Duration::from_secs(1)).await;
    assert!(broadcast.is_empty());

    auto_executor.unpause();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_multiple_unpause() {
    let queue = new_queue_typed::<SuccessAction>().await;
    let mut broadcast = queue.new_broadcast_receiver();

    queue.queue_action(SuccessAction {}).await.unwrap();

    let task_service = TaskService::new().unwrap();
    let auto_executor = queue
        .new_executor()
        .into_auto_executor(DummyWaitForOnline, &task_service);

    // Calling unpause should have no effect as auto executors starts unpaused.
    auto_executor.unpause();
    auto_executor.unpause();
    auto_executor.unpause();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_multiple_pause() {
    let queue = new_queue_typed::<SuccessAction>().await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_service = TaskService::new().unwrap();
    let auto_executor = queue
        .new_executor()
        .into_auto_executor(DummyWaitForOnline, &task_service);

    // Calling pause multiple times should still end up in paused state.
    auto_executor.pause();
    auto_executor.pause();
    auto_executor.pause();
    queue.queue_action(SuccessAction {}).await.unwrap();

    sleep(Duration::from_secs(1)).await;
    assert!(broadcast.is_empty());

    // Calling unpause multiple times should always end up in upaused state.
    auto_executor.unpause();
    auto_executor.unpause();
    auto_executor.unpause();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_pause_and_partially_manual_execution() {
    let queue = new_queue_typed::<SuccessAction>().await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_service = TaskService::new().unwrap();
    let auto_executor = queue
        .new_executor()
        .into_auto_executor(DummyWaitForOnline, &task_service);

    auto_executor.pause();
    queue.queue_action(SuccessAction {}).await.unwrap();
    queue.queue_action(SuccessAction {}).await.unwrap();

    sleep(Duration::from_secs(1)).await;
    assert!(broadcast.is_empty());

    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(output, QueuedActionState::Executed(_)),);

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));

    auto_executor.unpause();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
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

    assert!(matches!(
        output,
        QueuedActionState::Queued(_, QueuedActionReason::GuardExpired)
    ),);
}

#[tokio::test]
async fn execute_all_does_not_loop_forever_on_network_failure() {
    // There was a bug where execute all would loop forever if an action was re-queued due to
    // network failure.
    let queue = new_queue_typed::<ErrorAction>().await;

    let _ = queue.queue_action(ErrorAction {}).await.unwrap();

    queue.new_executor().execute_all().await.unwrap();
}

#[tokio::test]
async fn execute_all_waits_for_network_to_reoccur() {
    let is_offline = DeviceAlwaysOffline::default();
    let queue = new_queue_typed::<ErrorAction>().await;
    let mut broadcast = queue.new_broadcast_receiver();
    // We spawn an auto executor in the background.
    let task_service = TaskService::new().unwrap();
    let auto_executor = queue
        .new_executor()
        .into_auto_executor(is_offline.clone(), &task_service);

    auto_executor.pause();
    queue.queue_action(ErrorAction {}).await.unwrap();

    auto_executor.unpause();

    sleep(Duration::from_secs(5)).await;

    broadcast.recv().await.unwrap();

    // Check if the executor waited for the action.
    assert!(is_offline.0.load(std::sync::atomic::Ordering::Relaxed));
}

/// That implementation never returns, so the device is seen as always offline
#[derive(Clone, Default)]
struct DeviceAlwaysOffline(Arc<AtomicBool>);

impl WaitForOnlineSubscribtion for DeviceAlwaysOffline {
    fn subscribe(&self) -> impl WaitForOnline {
        self.clone()
    }
}
#[async_trait::async_trait]
impl WaitForOnline for DeviceAlwaysOffline {
    async fn wait_for_online(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        futures::future::pending::<()>().await;
    }
}

#[derive(Serialize, Deserialize)]
struct SuccessAction {}

impl Action for SuccessAction {
    const TYPE: Type = Type("success");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SuccessActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
struct SuccessActionHandler {}

impl Handler for SuccessActionHandler {
    type Action = SuccessAction;

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
        Ok(())
    }
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
