//! Async runtime abstraction.

use crate::runtime::tokio_runtime::TokioLocalSet;
use std::error::Error;
use std::future::Future;

#[cfg(feature = "tokio-runtime")]
mod tokio_runtime;

#[cfg(feature = "tokio-runtime")]
type RuntimeImpl = tokio_runtime::Runtime;

/// An async task that can be waited on.
pub trait JoinHandle<T>: Future<Output = Result<T, Box<dyn Error>>> {}

pub trait LocalTaskSet: Future<Output = ()> {
    fn spawn_local<R: 'static, F: Future<Output = R> + 'static>(&self, f: F) -> impl JoinHandle<R>;
}

/// Async runtime that runs on the current thread.
pub struct LocalRuntime(RuntimeImpl);
impl LocalRuntime {
    /// Create a new runtime that runs on the current thread.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        #[cfg(feature = "tokio-runtime")]
        Ok(Self(tokio_runtime::new_thread_local_runtime()?))
    }

    pub fn new_local_task_set() -> impl LocalTaskSet {
        #[cfg(feature = "tokio-runtime")]
        TokioLocalSet::new()
    }

    pub fn block_on<R, F: Future<Output = R>>(&self, f: F) -> R {
        #[cfg(feature = "tokio-runtime")]
        self.0.block_on(f)
    }
}

/// A multi-thread async runtime.
pub struct MTRuntime(RuntimeImpl);

impl MTRuntime {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        #[cfg(feature = "tokio-runtime")]
        Ok(Self(tokio_runtime::new_multi_thread_runtime()?))
    }
    pub fn spawn<R: Send + 'static, F: Future<Output = R> + Send + 'static>(
        &self,
        f: F,
    ) -> impl JoinHandle<R> {
        #[cfg(feature = "tokio-runtime")]
        tokio_runtime::TokioJoinHandle::new(&self.0, f)
    }

    pub fn block_on<R, F: Future<Output = R>>(&self, f: F) -> R {
        #[cfg(feature = "tokio-runtime")]
        self.0.block_on(f)
    }
}

#[test]
fn test_local_thread_runtime() {
    use std::time::Duration;

    let runtime = LocalRuntime::new().expect("failed to create runtime");

    runtime.block_on(async move {
        let task_set = LocalRuntime::new_local_task_set();
        let _ = task_set.spawn_local(async {
            crate::time::sleep(Duration::from_millis(100)).await;
        });
        task_set.await;
    });
}

#[test]
fn test_mt_runtime() {
    use std::time::Duration;

    let runtime = MTRuntime::new().expect("failed to create runtime");

    let h = runtime.spawn(async {
        crate::time::sleep(Duration::from_millis(100)).await;
    });

    runtime
        .block_on(async move { h.await })
        .expect("failed to wait");
}

pub fn spawn<R: Send + 'static, F: Future<Output = R> + Send + 'static>(
    f: F,
) -> impl JoinHandle<R> {
    #[cfg(feature = "tokio-runtime")]
    tokio_runtime::spawn(f)
}
