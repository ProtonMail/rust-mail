use crate::common::{new_factory, new_queue};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::queue::{QueueAutoExecutorPool, QueueAutoTerminationPolicy};
use proton_action_queue::tests::common::DefaultError;
use proton_task_service::TaskService;
use stash::stash::Bond;
use std::num::NonZeroUsize;
use std::time::Duration;
use tokio::sync::watch;

mod common;

#[tokio::test]
async fn auto_execute_until_empty() {
    let queue = new_queue(new_factory::<TestAction>()).await;
    let task_service = TaskService::new().unwrap();
    let online = watch::channel(true);

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();
    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 2);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        online.1,
        &task_service,
        QueueAutoTerminationPolicy::Empty,
    );

    executor.await_finished().await;
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_execute_until_network_failure() {
    let queue = new_queue(new_factory::<TestAction>()).await;
    let task_service = TaskService::new().unwrap();
    let online = watch::channel(true);

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    queue
        .queue_action(TestAction { fail_network: true })
        .await
        .unwrap();

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 3);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        online.1,
        &task_service,
        QueueAutoTerminationPolicy::NetworkLoss,
    );

    executor.await_finished().await;
    assert_eq!(queue.queued_actions_count().await.unwrap(), 2);
}

#[tokio::test]
async fn auto_execute_until_empty_or_network_failure() {
    let queue = new_queue(new_factory::<TestAction>()).await;
    let task_service = TaskService::new().unwrap();
    let online = watch::channel(true);

    let action_id = queue
        .queue_action(TestAction { fail_network: true })
        .await
        .unwrap()
        .id;

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 3);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        online.1.clone(),
        &task_service,
        QueueAutoTerminationPolicy::EmptyOrNetworkLoss,
    );

    executor.await_finished().await;
    assert_eq!(queue.queued_actions_count().await.unwrap(), 3);

    // Delete action that triggers network failures.
    queue.delete_action(action_id).await.unwrap();
    assert_eq!(queue.queued_actions_count().await.unwrap(), 2);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        online.1,
        &task_service,
        QueueAutoTerminationPolicy::EmptyOrNetworkLoss,
    );

    executor.await_finished().await;
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_execute_pool() {
    let queue = new_queue(new_factory::<TestAction>()).await;
    let task_service = TaskService::new().unwrap();
    let online = watch::channel(true);

    for _ in 0..20 {
        queue
            .queue_action(TestAction {
                fail_network: false,
            })
            .await
            .unwrap();
    }

    assert_eq!(queue.queued_actions_count().await.unwrap(), 20);

    let executor_pool = QueueAutoExecutorPool::with_termination_policy(
        &queue,
        &ActionGroup::default(),
        NonZeroUsize::new(3).unwrap(),
        online.1,
        &task_service,
        QueueAutoTerminationPolicy::Empty,
    );

    // This test can take up to 1 min to complete due to the timeout while waiting for external
    // changes. To be improved.
    tokio::time::timeout(Duration::from_secs(70), executor_pool.await_finished())
        .await
        .unwrap();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_execute_forever() {
    let queue = new_queue(new_factory::<TestAction>()).await;
    let task_service = TaskService::new().unwrap();
    let online = watch::channel(true);

    queue
        .queue_action(TestAction {
            fail_network: false,
        })
        .await
        .unwrap();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 1);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        online.1,
        &task_service,
        QueueAutoTerminationPolicy::Never,
    );

    tokio::time::timeout(Duration::from_millis(100), executor.await_finished())
        .await
        .unwrap_err();

    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestAction {
    fail_network: bool,
}

impl Action for TestAction {
    const TYPE: Type = Type("TEST_ACTION");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = TestHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
struct TestHandler;

impl Handler for TestHandler {
    type Action = TestAction;
    type Context = ();

    async fn apply_local(
        &self,
        _: ActionId,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // do nothing
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        (): &Self::Context,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if action.fail_network {
            return Err(DefaultError::NetworkFailure);
        }
        Ok(())
    }
}
