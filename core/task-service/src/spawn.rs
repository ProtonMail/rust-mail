use crate::service::TaskService;
use std::future::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub enum AsyncTaskResult<T: Send> {
    Completed(T),
    Cancelled,
}

pub trait TaskSpawner {
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

/// Spawn a task that races with given cancellation token.
pub fn spawn_task<F, S>(
    tasks: &TaskService,
    token: CancellationToken,
    future: F,
) -> JoinHandle<AsyncTaskResult<F::Output>>
where
    F: Future + Send + 'static,
    <F as Future>::Output: Send,
    S: TaskSpawner + 'static,
{
    S::spawn::<_>(cancelable_task(token, tasks.guard(future)))
}

async fn cancelable_task<F: Future>(
    token: CancellationToken,
    future: F,
) -> AsyncTaskResult<F::Output>
where
    F::Output: Send,
{
    tokio::select! {
        () = token.cancelled() => AsyncTaskResult::Cancelled,
        r = future =>  AsyncTaskResult::Completed(r),
    }
}
