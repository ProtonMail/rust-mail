mod common;

use crate::common::{new_queue_typed, DefaultError};
use proton_action_queue::action::{Action, DefaultVersionConverter, Handler, Type};
use proton_action_queue::observers::{ActionAwaiter, ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::BroadcastMessage;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Stash};
use std::future::Future;
use std::time::Duration;

#[tokio::test]
async fn failure_action_observer_remote() {
    let queue = new_queue_typed::<ErrorAction>().await;
    queue.register::<SuccessAction>().unwrap();

    let id_cancel = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_delete = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_execute = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let _ = queue.queue_action(SuccessAction {}).await.unwrap();

    let mut error_observer = ActionFailureObserver::<ErrorAction>::new(&queue);
    let mut success_observer = ActionFailureObserver::<SuccessAction>::new(&queue);

    // check cancelled response.
    queue.cancel(id_cancel).await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = error_observer.next() => {
            r
        }
    }
    .unwrap();
    if let ActionFailureReason::Cancelled(metadata) = result {
        assert_eq!(id_cancel, metadata.id);
    } else {
        panic!("Expected cancellation reason");
    }

    // check delete response.
    queue.delete_action(id_delete).await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = error_observer.next() => {
            r
        }
    }
    .unwrap();
    if let ActionFailureReason::Deleted(id) = result {
        assert_eq!(id_delete, id);
    } else {
        panic!("Expected cancellation reason");
    }

    // check execution failure
    queue.execute_one().await.unwrap_err();
    // execute success action.
    queue.execute_one().await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = error_observer.next() => {
            r
        }
    }
    .unwrap();
    if let ActionFailureReason::Error(_, metadata) = result {
        assert_eq!(id_execute, metadata.id);
    } else {
        panic!("Expected execution failure reason");
    }

    tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(1)) => {}
        _ = success_observer.next() => {
            panic!("We should never receive anything")
        }
    }
}

#[tokio::test]
async fn action_awaiter() {
    let queue = new_queue_typed::<ErrorAction>().await;
    queue.register::<SuccessAction>().unwrap();

    let id_cancel = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_delete = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_error = queue.queue_action(ErrorAction {}).await.unwrap().id;
    let id_success = queue.queue_action(SuccessAction {}).await.unwrap().id;

    let cancel_awaiter = ActionAwaiter::new(&queue, id_cancel);
    let delete_awaiter = ActionAwaiter::new(&queue, id_delete);
    let error_awaiter = ActionAwaiter::new(&queue, id_error);
    let success_awaiter = ActionAwaiter::new(&queue, id_success);

    // check cancelled response.
    queue.cancel(id_cancel).await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = cancel_awaiter.wait() => {
            r
        }
    }
    .unwrap();
    assert!(matches!(result, BroadcastMessage::Cancelled(_)));

    // check delete response.
    queue.delete_action(id_delete).await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = delete_awaiter.wait() => {
            r
        }
    }
    .unwrap();
    assert!(matches!(result, BroadcastMessage::Deleted(_, _)));

    // check execution failure
    queue.execute_one().await.unwrap_err();

    // check failure execution.
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = error_awaiter.wait() => {
            r
        }
    }
    .unwrap();
    assert!(matches!(result, BroadcastMessage::Error(_, _)));

    // execute success action.
    queue.execute_one().await.unwrap();
    let result = tokio::select! {
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Timeout expired");
        }
        r = success_awaiter.wait() => {
            r
        }
    }
    .unwrap();
    assert!(matches!(result, BroadcastMessage::Success(_)));
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorAction {}

impl Action for ErrorAction {
    const TYPE: Type = Type("remote_error_action");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<ErrorAction>;
    type Handler = ErrorActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
pub struct ErrorActionHandler {}

impl Handler for ErrorActionHandler {
    type Action = ErrorAction;
    type Context = ();

    fn apply_local(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error>,
    > + Send {
        std::future::ready(Ok(()))
    }

    fn revert_local(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond,
    ) -> impl Future<Output = Result<(), <Self::Action as Action>::Error>> + Send {
        std::future::ready(Ok(()))
    }

    fn apply_remote(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Stash,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error>,
    > + Send {
        std::future::ready(Err(DefaultError::APIFailure))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessAction {}

impl Action for SuccessAction {
    const TYPE: Type = Type("success_action");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<SuccessAction>;
    type Handler = SuccessActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = DefaultError;
    type Context = ();
}

#[derive(Default)]
pub struct SuccessActionHandler {}

impl Handler for SuccessActionHandler {
    type Action = SuccessAction;
    type Context = ();

    fn apply_local(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error>,
    > + Send {
        std::future::ready(Ok(()))
    }

    fn revert_local(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Bond,
    ) -> impl Future<Output = Result<(), <Self::Action as Action>::Error>> + Send {
        std::future::ready(Ok(()))
    }

    fn apply_remote(
        &self,
        (): &Self::Context,
        _: &mut Self::Action,
        _: &Stash,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error>,
    > + Send {
        std::future::ready(Ok(()))
    }
}
