use crate::IntoPausableFuture;
use crate::service::TaskService;
use std::future::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Async task result.
pub enum AsyncTaskResult<T: Send> {
    /// The task completed it is execution and this is the result.
    Completed(T),
    /// The task was cancelled due to user request.
    Cancelled,
}

/// Abstraction trait to abstract the async task spawning.
pub trait TaskSpawner {
    /// Spawn the given task on the runtime.
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F::Output: Send + 'static,
        F: Future + Send + 'static;
}

pub struct DefaultTaskSpawner;
impl TaskSpawner for DefaultTaskSpawner {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F::Output: Send + 'static,
        F: Future + Send + 'static,
    {
        tokio::spawn(f)
    }
}

/// Spawn an async `task` tied to a `cancellation_token`.
///
/// The `task` will be spawned in a race with the `cancellation_token`.
///
/// If the tasks completes before it gets cancelled, the output will be returned with
///[`AsyncTaskResult::Completed`], otherwise [`AsyncTaskResult::Cancelled` will be returned.
pub fn spawn_task<F, S>(
    task_service: &TaskService,
    cancellation_token: CancellationToken,
    task: F,
) -> JoinHandle<AsyncTaskResult<F::Output>>
where
    F: Future + Send + 'static,
    <F as Future>::Output: Send,
    S: TaskSpawner + 'static,
{
    S::spawn::<_>(cancelable_task(
        cancellation_token,
        task.into_pausable(task_service),
    ))
}

/// Utility wrapper that races a `task` against a `cancellation_token`.
pub async fn cancelable_task<F: Future>(
    cancellation_token: CancellationToken,
    task: F,
) -> AsyncTaskResult<F::Output>
where
    F::Output: Send,
{
    tokio::select! {
        () = cancellation_token.cancelled() => {
            AsyncTaskResult::Cancelled
        }

        r = task => {
            AsyncTaskResult::Completed(r)
        }
    }
}
