use super::common::{new_factory, new_queue};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::queue::{
    ActionRequeueReason, NoopOnlineStatusWaiter, QueueAutoTerminationPolicy, QueuedActionState,
    QueuedError, TokioTaskSpawner,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_action_queue::tests::common::DefaultError;
use stash::stash::Bond;

#[tokio::test]
async fn execute_action_with_max_retries_on_network_failure() {
    let queue = new_queue(new_factory::<TestAction>(TestHandler)).await;

    queue
        .queue_action(TestAction { fail_network: true })
        .await
        .unwrap();
    assert_eq!(queue.queued_actions_count().await.unwrap(), 1);

    let executor = queue.new_executor();
    assert!(matches!(
        executor.execute_one().await,
        Ok(Some(QueuedActionState::Queued(
            _,
            ActionRequeueReason::NetworkFailed
        )))
    ));
    assert!(matches!(
        executor.execute_one().await,
        Ok(Some(QueuedActionState::Queued(
            _,
            ActionRequeueReason::NetworkFailed
        )))
    ));
    assert!(matches!(
        executor.execute_one().await,
        Ok(Some(QueuedActionState::Queued(
            _,
            ActionRequeueReason::NetworkFailed
        )))
    ));
    assert!(matches!(
        executor.execute_one().await,
        Err(QueuedError::Action(_, _))
    ));
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[tokio::test]
async fn auto_execute_with_max_retries_on_network_failure_wont_block_the_queue() {
    let queue = new_queue(new_factory::<TestAction>(TestHandler)).await;
    let task_spawner = TokioTaskSpawner;

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

    assert_eq!(queue.queued_actions_count().await.unwrap(), 2);

    let executor = queue.new_executor().into_auto_executor_with_policy(
        Box::new(NoopOnlineStatusWaiter),
        false,
        &task_spawner,
        QueueAutoTerminationPolicy::Empty,
        tracing::Span::current(),
    );

    executor.await_finished().await;
    assert_eq!(queue.queued_actions_count().await.unwrap(), 0);
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestAction {
    fail_network: bool,
}

impl Action for TestAction {
    const TYPE: Type = Type("TEST_ACTION");
    const VERSION: u32 = 1;
    const MAX_RETRIES: Option<u32> = Some(3);

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = TestHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
struct TestHandler;

impl Handler for TestHandler {
    type Action = TestAction;

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
        // do nothing
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if action.fail_network {
            return Err(DefaultError::NetworkFailure);
        }
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
