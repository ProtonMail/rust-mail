//! Async runtime abstraction.
use crate::runtime::tokio_runtime::TokioLocalSet;
use pin_project::pin_project;
use std::error::Error;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(feature = "tokio-runtime")]
mod tokio_runtime;

#[cfg(feature = "tokio-runtime")]
type RuntimeImpl = tokio_runtime::Runtime;

/// An async task that can be waited on.
#[pin_project]
pub struct RuntimeJoinHandle<R, E: Into<Box<dyn Error>>, F: Future<Output = Result<R, E>>> {
    #[pin]
    f: F,
    p: PhantomData<(R, E)>,
}

impl<R, E: Into<Box<dyn Error>>, F: Future<Output = Result<R, E>>> RuntimeJoinHandle<R, E, F> {
    fn new(f: F) -> Self {
        Self { f, p: PhantomData }
    }
}

impl<R, E: Into<Box<dyn Error>>, F: Future<Output = Result<R, E>>> Future
    for RuntimeJoinHandle<R, E, F>
{
    type Output = Result<R, Box<dyn Error>>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().f.poll(ctx).map_err(|e| e.into())
    }
}

pub type JoinHandle<R> = RuntimeJoinHandle<R, tokio::task::JoinError, tokio::task::JoinHandle<R>>;

pub trait LocalTaskSetSpawn {
    fn spawn_local<R: 'static, F: Future<Output = R> + 'static>(&self, f: F) -> JoinHandle<R>;
}

#[pin_project]
pub struct RuntimeLocalTaskSet<T: LocalTaskSetSpawn + Future<Output = ()>> {
    #[pin]
    t: T,
}

impl<T: LocalTaskSetSpawn + Future<Output = ()>> RuntimeLocalTaskSet<T> {
    fn new(t: T) -> Self {
        Self { t }
    }

    pub fn spawn_local<R: 'static, F: Future<Output = R> + 'static>(&self, f: F) -> JoinHandle<R> {
        self.t.spawn_local(f)
    }
}

impl<T: LocalTaskSetSpawn + Future<Output = ()>> Future for RuntimeLocalTaskSet<T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().t.poll(cx)
    }
}

#[cfg(feature = "tokio-runtime")]
pub type LocalTaskSet = RuntimeLocalTaskSet<TokioLocalSet>;

/// Async runtime that runs on the current thread.
pub struct LocalRuntime(RuntimeImpl);
impl LocalRuntime {
    /// Create a new runtime that runs on the current thread.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        #[cfg(feature = "tokio-runtime")]
        Ok(Self(tokio_runtime::new_thread_local_runtime()?))
    }

    pub fn new_local_task_set() -> LocalTaskSet {
        #[cfg(feature = "tokio-runtime")]
        LocalTaskSet::new(TokioLocalSet::new())
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
    ) -> JoinHandle<R> {
        #[cfg(feature = "tokio-runtime")]
        RuntimeJoinHandle::new(self.0.spawn(f))
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

pub fn spawn<R: Send + 'static, F: Future<Output = R> + Send + 'static>(f: F) -> JoinHandle<R> {
    #[cfg(feature = "tokio-runtime")]
    JoinHandle::new(tokio::spawn(f))
}
