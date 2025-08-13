use std::{
    future::Future,
    mem,
    pin::Pin,
    sync::{Arc, Weak},
};
use tokio::task::JoinHandle;

pub enum AsyncTaskResult<T> {
    Completed(T),
    Cancelled,
}

pub trait Runtime {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static;
}

pub struct Tokio;

impl Tokio {
    #[must_use]
    #[doc(hidden)]
    pub fn weak() -> Weak<Self> {
        let this = Arc::new(Self);
        let this_weak = Arc::downgrade(&this);

        mem::forget(this);

        this_weak
    }
}

impl Runtime for Tokio {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        tokio::spawn(f)
    }
}

pub trait Spawner
where
    Self: Send + Sync,
{
    fn spawn_task<F>(&self, f: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future<Output: Send> + Send + 'static;
}

/// [`Spawner`] that's dyn-compatible (to use as `Weak<dyn DynSpawner>`, for
/// instance)
pub trait DynSpawner
where
    Self: Send + Sync,
{
    fn spawn_boxed_task(
        &self,
        f: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> JoinHandle<AsyncTaskResult<()>>;
}

impl<T> DynSpawner for T
where
    T: Spawner + ?Sized,
{
    fn spawn_boxed_task(
        &self,
        f: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> JoinHandle<AsyncTaskResult<()>> {
        self.spawn_task(f)
    }
}

impl DynSpawner for Tokio {
    fn spawn_boxed_task(
        &self,
        f: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> JoinHandle<AsyncTaskResult<()>> {
        tokio::spawn(async move {
            f.await;
            AsyncTaskResult::Completed(())
        })
    }
}

pub type WeakSpawner = Weak<dyn DynSpawner>;
