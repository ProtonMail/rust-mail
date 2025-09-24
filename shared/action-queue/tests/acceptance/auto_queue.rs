use super::common::DefaultError;
use super::common::new_queue_typed;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard, WriterGuardError,
};
use proton_action_queue::queue::{
    ActionRequeueReason, BroadcastMessage, NoopOnlineStatusWaiter, OnlineStatusWaiter,
    QueuedActionState, TokioTaskSpawner,
};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn auto_queued_on_network_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let queue = new_queue_typed::<ErrorAction>(ErrorActionHandler).await;

    queue.queue_action(ErrorAction).await.unwrap();

    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(
        output,
        QueuedActionState::Queued(_, ActionRequeueReason::NetworkFailed)
    ));
}

#[tokio::test]
async fn auto_queued_on_pause() {
    let queue = new_queue_typed::<SuccessAction>(SuccessActionHandler).await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_spawner = TokioTaskSpawner;

    let auto_executor = queue.new_executor().into_auto_executor(
        Box::new(NoopOnlineStatusWaiter),
        false,
        &task_spawner,
        tracing::Span::current(),
    );

    auto_executor.pause();
    queue.queue_action(SuccessAction).await.unwrap();

    assert!(matches!(
        broadcast.recv().await.unwrap(),
        BroadcastMessage::Queued(_, _)
    ));
    sleep(Duration::from_secs(1)).await;
    assert!(broadcast.is_empty());

    auto_executor.resume();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_multiple_resume() {
    let queue = new_queue_typed::<SuccessAction>(SuccessActionHandler).await;
    let mut broadcast = queue.new_broadcast_receiver();

    queue.queue_action(SuccessAction).await.unwrap();

    assert!(matches!(
        broadcast.recv().await.unwrap(),
        BroadcastMessage::Queued(_, _)
    ));

    let task_spawner = TokioTaskSpawner;

    let auto_executor = queue.new_executor().into_auto_executor(
        Box::new(NoopOnlineStatusWaiter),
        false,
        &task_spawner,
        tracing::Span::current(),
    );

    // Calling resume should have no effect as auto executors starts active.
    auto_executor.resume();
    auto_executor.resume();
    auto_executor.resume();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_multiple_pause() {
    let queue = new_queue_typed::<SuccessAction>(SuccessActionHandler).await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_spawner = TokioTaskSpawner;

    let auto_executor = queue.new_executor().into_auto_executor(
        Box::new(NoopOnlineStatusWaiter),
        false,
        &task_spawner,
        tracing::Span::current(),
    );

    // Calling pause multiple times should still end up in paused state.
    auto_executor.pause();
    auto_executor.pause();
    auto_executor.pause();
    queue.queue_action(SuccessAction).await.unwrap();

    sleep(Duration::from_secs(1)).await;
    assert!(matches!(
        broadcast.recv().await.unwrap(),
        BroadcastMessage::Queued(_, _)
    ));

    // Calling unpause multiple times should always end up in upaused state.
    auto_executor.resume();
    auto_executor.resume();
    auto_executor.resume();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_pause_and_partially_manual_execution() {
    let queue = new_queue_typed::<SuccessAction>(SuccessActionHandler).await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_spawner = TokioTaskSpawner;

    let auto_executor = queue.new_executor().into_auto_executor(
        Box::new(NoopOnlineStatusWaiter),
        false,
        &task_spawner,
        tracing::Span::current(),
    );

    auto_executor.pause();
    queue.queue_action(SuccessAction).await.unwrap();
    queue.queue_action(SuccessAction).await.unwrap();

    assert!(matches!(
        broadcast.recv().await.unwrap(),
        BroadcastMessage::Queued(_, _)
    ));
    assert!(matches!(
        broadcast.recv().await.unwrap(),
        BroadcastMessage::Queued(_, _)
    ));
    sleep(Duration::from_secs(1)).await;
    assert!(broadcast.is_empty());

    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(output, QueuedActionState::Executed(_)),);

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));

    auto_executor.resume();

    let output = broadcast.recv().await.unwrap();

    assert!(matches!(output, BroadcastMessage::Success(_)));
    assert!(broadcast.is_empty());
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_queued_on_writer_guard_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let queue = new_queue_typed::<WriteGuardExpiredAction>(WriterGuardExpiredActionHandler).await;

    queue
        .queue_action(WriteGuardExpiredAction {})
        .await
        .unwrap();

    let output = queue.new_executor().execute_one().await.unwrap().unwrap();

    assert!(matches!(
        output,
        QueuedActionState::Queued(_, ActionRequeueReason::GuardExpired)
    ),);
}

#[tokio::test]
async fn execute_all_does_not_loop_forever_on_network_failure() {
    // There was a bug where execute all would loop forever if an action was re-queued due to
    // network failure.
    let queue = new_queue_typed::<ErrorAction>(ErrorActionHandler).await;

    let _ = queue.queue_action(ErrorAction).await.unwrap();

    queue.new_executor().execute_all().await.unwrap();
}

#[tokio::test]
async fn execute_all_waits_for_network_to_reoccur() {
    struct TimedOnlineStatusWaiter(Duration);

    #[async_trait::async_trait]
    impl OnlineStatusWaiter for TimedOnlineStatusWaiter {
        async fn wait_until_online(&mut self) {
            tokio::time::sleep(self.0).await;
        }
    }
    let queue = new_queue_typed::<ErrorAction>(ErrorActionHandler).await;
    let mut broadcast = queue.new_broadcast_receiver();
    let task_spawner = TokioTaskSpawner;

    let auto_executor = queue.new_executor().into_auto_executor(
        Box::new(TimedOnlineStatusWaiter(Duration::from_secs(2))),
        false,
        &task_spawner,
        tracing::Span::current(),
    );

    auto_executor.pause();
    queue.queue_action(ErrorAction).await.unwrap();
    auto_executor.resume();

    sleep(Duration::from_secs(5)).await;

    broadcast.recv().await.unwrap();
}

#[derive(Serialize, Deserialize)]
struct SuccessAction;

impl Action for SuccessAction {
    const TYPE: Type = Type("success");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SuccessActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
struct SuccessActionHandler;

impl Handler for SuccessActionHandler {
    type Action = SuccessAction;

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
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct ErrorAction;

impl Action for ErrorAction {
    const TYPE: Type = Type("error");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ErrorActionHandler;
    type RemoteOutput = u32;
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
struct ErrorActionHandler;

impl Handler for ErrorActionHandler {
    type Action = ErrorAction;

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
}

#[derive(Default)]
struct WriterGuardExpiredActionHandler;

impl Handler for WriterGuardExpiredActionHandler {
    type Action = WriteGuardExpiredAction;

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
        Err(WriterGuardError::Expired.into())
    }
}
