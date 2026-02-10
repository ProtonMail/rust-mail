use futures::future::BoxFuture;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock, Weak},
};
use tokio::task::JoinHandle;
use tracing::warn;

/// An abstraction over [`tokio::spawn()`] that allows to spawn managed tasks.
///
/// Contract is quite simple here - tasks spawned through `spawn_task()` are, in
/// general, supposed to be bound lifetime-wise to the spawner itself, so that:
///
/// - if the spawner gets dropped, tasks spawned on that spawner are supposed to
///   be cancelled,
///
/// - if the spawner gets paused, tasks spawned on that spawner are supposed to
///   be paused.
///
/// This should be preferred over [`tokio::spawn()`], since it allows us to
/// properly pause futures when the application goes to background, cancel them
/// when user gets logged out etc.
///
/// See also: [`DynSpawner`], [`SpawnerRef`].
pub trait Spawner
where
    Self: Send + Sync,
{
    fn spawn_task<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static;
}

/// A dyn-compatible [`Spawner`].
pub trait DynSpawner
where
    Self: Send + Sync,
{
    fn spawn_boxed_task(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle<()>;
}

impl<T> DynSpawner for T
where
    T: Spawner + ?Sized,
{
    fn spawn_boxed_task(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle<()> {
        self.spawn_task(f)
    }
}

#[derive(Clone)]
pub struct SpawnerRef(Weak<dyn DynSpawner>);

impl SpawnerRef {
    #[must_use]
    pub fn new(spawner: Weak<dyn DynSpawner>) -> Self {
        Self(spawner)
    }
}

impl DynSpawner for SpawnerRef {
    fn spawn_boxed_task(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle<()> {
        if let Some(spawner) = self.0.upgrade() {
            spawner.spawn_boxed_task(f)
        } else {
            warn!("Tried to spawn a task onto a cancelled spawner");

            let task = tokio::spawn(std::future::pending());

            task.abort();
            task
        }
    }
}

impl muon::rt::Spawner for SpawnerRef {
    fn spawn(&self, fut: BoxFuture<'static, ()>) {
        self.spawn_boxed_task(fut);
    }
}

/// See [`Tokio::spawner()`].
#[doc(hidden)]
pub struct Tokio {
    _priv: (),
}

impl Tokio {
    /// A Tokio-based [`Spawner`].
    ///
    /// This is a wrapper for [`tokio::spawn()`] - the idea is that you'd call
    /// [`Tokio::spawner()`] whenever you have something that needs an instance
    /// of [`Spawner`], but you don't have one at hand.
    ///
    /// Since most of the tasks should be bound to user context, mail context or
    /// anything else you have at hand, this function should be used very rarely
    /// (hence `#[doc(hidden)]`).
    ///
    /// There are just a couple of instances in our code where we have to spawn
    /// something before a real context is ready etc.
    #[must_use]
    #[doc(hidden)]
    pub fn spawner() -> SpawnerRef {
        static THIS: LazyLock<Arc<Tokio>> = LazyLock::new(|| Arc::new(Tokio { _priv: () }));

        #[allow(trivial_casts, reason = "false-positive")]
        SpawnerRef::new(Arc::downgrade(&*THIS) as _)
    }
}

impl Spawner for Tokio {
    fn spawn_task<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        tokio::spawn(f)
    }
}
