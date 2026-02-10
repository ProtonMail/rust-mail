use crate::common::BoxFut;
use crate::rt::Spawner;
use async_channel::{unbounded, Receiver, Sender};
use async_executor::Executor;
use futures::executor::block_on;
use futures::prelude::*;
use lazy_static::lazy_static;
use std::num::NonZeroUsize;
use std::pin::pin;
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};

lazy_static! {
    /// A global async executor instance.
    ///
    /// This instance is constructed on first use.
    /// If finer control is needed, a custom spawner can be created
    /// (see [`AsyncExecutor::new`]).
    static ref EXECUTOR: AsyncExecutor = AsyncExecutor::default();
}

/// An async executor instance.
///
/// This type can be used to drive async futures to completion.
/// To spawn futures, an [`AsyncSpawner`] must be obtained from the executor.
/// The handle is designed to be cheaply cloneable and can be passed around
/// as needed.
#[must_use]
#[derive(Debug)]
pub struct AsyncExecutor {
    /// The inner executor, shared across all worker threads.
    exec: Arc<Executor<'static>>,

    /// The sender side of the shutdown channel.
    ///
    /// When dropped, the worker threads (which are waiting on the receiver
    /// side) will be notified to shut down, and any handles to the executor
    /// will no longer be able to spawn futures.
    #[allow(unused)]
    tx: Sender<()>,

    /// The receiver side of the shutdown channel.
    ///
    /// This is used to notify worker threads to shut down.
    rx: Receiver<()>,
}

impl Default for AsyncExecutor {
    fn default() -> Self {
        if let Some(n) = NonZeroUsize::new(num_cpus::get() - 1) {
            Self::new(n)
        } else {
            Self::new(NonZeroUsize::MIN)
        }
    }
}

impl AsyncExecutor {
    /// Create a new executor with the given number of worker threads.
    pub fn new(n: NonZeroUsize) -> Self {
        let (tx, rx) = unbounded();

        let exec = Arc::new(Executor::new());

        for id in 0..n.get() {
            run_worker(id, exec.clone(), rx.clone());
        }

        Self { exec, tx, rx }
    }

    /// Acquire a handle to the executor.
    ///
    /// The handle can be used to spawn futures for background execution.
    pub fn handle(&self) -> AsyncSpawner {
        AsyncSpawner {
            exec: Arc::downgrade(&self.exec),
            rx: self.rx.clone(),
        }
    }
}

/// A handle to an async executor, used to spawn futures.
#[must_use]
#[derive(Debug, Clone)]
pub struct AsyncSpawner {
    exec: Weak<Executor<'static>>,
    rx: Receiver<()>,
}

/// Creates a handle to the global async executor.
///
/// If finer control is needed, a custom executor can be created
/// and a handle obtained from it.
impl Default for AsyncSpawner {
    fn default() -> Self {
        EXECUTOR.handle()
    }
}

impl AsyncSpawner {
    /// Spawn the given future for background execution.
    ///
    /// Returns a oneshot receiver that will receive the future's output.
    pub fn spawn<F>(&self, fut: F)
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let fut = async move {
            trace!("running spawned future");
            drop(fut.await);
            trace!("spawned future finished");
        };

        if !self.rx.is_closed() {
            if let Some(exec) = self.exec.upgrade() {
                exec.spawn(fut).detach();
            }
        }
    }
}

fn run_worker(id: usize, ex: Arc<Executor<'static>>, rx: Receiver<()>) -> JoinHandle<()> {
    trace!(%id, "spawning worker thread");

    thread::Builder::new()
        .name(format!("muon-{id}"))
        .spawn(move || {
            trace_span!("worker", %id).in_scope(move || {
                let mut rx = pin!(rx);

                trace!("running worker thread");
                block_on(ex.run(rx.next()));
                trace!("stopped worker thread");
            });
        })
        .expect("spawn should succeed")
}

impl Spawner for AsyncSpawner {
    fn spawn(&self, fut: BoxFut<'static, ()>) {
        self.spawn(fut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::oneshot::{self, Canceled};

    #[test]
    fn test_async_executor() {
        let (tx, rx) = oneshot::channel();

        // Get the global executor handle.
        let s = AsyncSpawner::default();

        // Spawn a future that sends a value on a oneshot channel.
        s.spawn(async {
            let _ = tx.send(42);
        });

        // The future should resolve to the sent value.
        assert_eq!(block_on(rx), Ok(42));
    }

    #[test]
    fn test_async_executor_drop() {
        let (tx, rx) = oneshot::channel();

        // Create a new executor and its handle.
        let e = AsyncExecutor::default();
        let s = e.handle();

        // Drop the executor while the handle is still alive.
        drop(e);

        // Attempt to spawn a future; this should not panic.
        s.spawn(async {
            let _ = tx.send(42);
        });

        // The future should not resolve.
        assert_eq!(block_on(rx), Err(Canceled));
    }
}
