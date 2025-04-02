use std::future::Future;
use tokio::task::JoinHandle;

/// Abstraction over the underlying runtime.
pub trait TaskSpawner {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static;
}

pub struct DefaultTaskSpawner;

impl TaskSpawner for DefaultTaskSpawner {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        tokio::spawn(f)
    }
}

/// Outcome of a cancellable task.
pub enum AsyncTaskResult<T> {
    Completed(T),
    Cancelled,
}
