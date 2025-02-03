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

/// Spawn an async `task` tied to a `cancellation_token`.
///
/// The `task` will be spawned in a race with the `cancellation_token`.
///
/// If the tasks completes before it gets cancelled, the output will be returned with
///[`AsyncTaskResult::Completed`], otherwise [`AsyncTaskResult::Cancelled` will be returned.
pub fn spawn_task<T: Send + 'static>(
    cancellation_token: CancellationToken,
    task: impl Future<Output = T> + Send + 'static,
) -> JoinHandle<AsyncTaskResult<T>> {
    tokio::spawn(async move {
        tokio::select! {
            () = cancellation_token.cancelled() => {
                AsyncTaskResult::Cancelled
            }

            r = task => {
                AsyncTaskResult::Completed(r)
            }
        }
    })
}
