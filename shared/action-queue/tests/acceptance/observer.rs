use super::common::{DefaultError, new_queue_typed};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::observers::{ActionAwaiter, ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::BroadcastMessage;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::time::Duration;

#[tokio::test]
async fn failure_action_observer_remote() {
    let queue = new_queue_typed::<ErrorAction>(ErrorActionHandler).await;

    queue
        .register::<SuccessAction>(SuccessActionHandler)
        .unwrap();

    let executor = queue.new_executor();

    let id_cancel = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_delete = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_execute = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let _ = queue.queue_action(SuccessAction {}).await.unwrap();

    let mut error_observer = ActionFailureObserver::<ErrorAction>::new(&queue);
    let mut success_observer = ActionFailureObserver::<SuccessAction>::new(&queue);

    // check cancelled response.
    queue.cancel(id_cancel).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), error_observer.next())
        .await
        .expect("timed out")
        .unwrap();
    if let ActionFailureReason::Cancelled(metadata) = result {
        assert_eq!(id_cancel, metadata.id);
    } else {
        panic!("Expected cancellation reason");
    }

    // check delete response.
    queue.delete_action(id_delete).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), error_observer.next())
        .await
        .expect("timed out")
        .unwrap();
    if let ActionFailureReason::Deleted(id) = result {
        assert_eq!(id_delete, id);
    } else {
        panic!("Expected cancellation reason");
    }

    // check execution failure
    executor.execute_one().await.unwrap_err();
    // execute success action.
    executor.execute_one().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), error_observer.next())
        .await
        .expect("timed out")
        .unwrap();
    if let ActionFailureReason::Error(_, metadata) = result {
        assert_eq!(id_execute, metadata.id);
    } else {
        panic!("Expected execution failure reason");
    }

    tokio::time::timeout(Duration::from_secs(1), success_observer.next())
        .await
        .expect_err("should time out");
}

#[tokio::test]
async fn action_awaiter() {
    let queue = new_queue_typed::<ErrorAction>(ErrorActionHandler).await;

    queue
        .register::<SuccessAction>(SuccessActionHandler)
        .unwrap();

    let id_cancel = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_delete = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_error = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_success = queue.queue_action(SuccessAction {}).await.unwrap().id;

    let mut cancel_awaiter = ActionAwaiter::new(&queue);
    let mut delete_awaiter = ActionAwaiter::new(&queue);
    let mut error_awaiter = ActionAwaiter::new(&queue);
    let mut success_awaiter = ActionAwaiter::new(&queue);

    // check cancelled response.
    queue.cancel(id_cancel).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), cancel_awaiter.wait(id_cancel))
        .await
        .expect("timed out")
        .unwrap();
    assert!(matches!(result, BroadcastMessage::Cancelled(_)));

    // check delete response.
    queue.delete_action(id_delete).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), delete_awaiter.wait(id_delete))
        .await
        .expect("timed out")
        .unwrap();
    assert!(matches!(result, BroadcastMessage::Deleted(_, _)));

    let executor = queue.new_executor();
    // check execution failure
    executor.execute_one().await.unwrap_err();

    // check failure execution.
    queue.delete_action(id_delete).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), error_awaiter.wait(id_error))
        .await
        .expect("timed out")
        .unwrap();
    assert!(matches!(result, BroadcastMessage::Error(_, _)));

    // execute success action.
    executor.execute_one().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), success_awaiter.wait(id_success))
        .await
        .expect("timed out")
        .unwrap();
    assert!(matches!(result, BroadcastMessage::Success(_, _)));
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorAction;

impl Action for ErrorAction {
    const TYPE: Type = Type("remote_error_action");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<ErrorAction>;
    type Handler = ErrorActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
pub struct ErrorActionHandler;

impl Handler for ErrorActionHandler {
    type Action = ErrorAction;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
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

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessAction;

impl Action for SuccessAction {
    const TYPE: Type = Type("success_action");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<SuccessAction>;
    type Handler = SuccessActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
}

#[derive(Default)]
pub struct SuccessActionHandler;

impl Handler for SuccessActionHandler {
    type Action = SuccessAction;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        Ok(())
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
