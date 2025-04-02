use crate::TaskSpawner;
use crate::spawn::DefaultTaskSpawner;
use pin_project::pin_project;
use std::cell::Cell;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::task::{Context, Poll, Waker};
use std::thread;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace};

thread_local! {
    static DRAIN: Cell<bool> = const { Cell::new(false) };
}

/// Manages futures that can be paused on demand.
pub struct TaskService {
    active: Arc<AtomicBool>,
    sender: mpsc::Sender<Command>,
}

impl TaskService {
    /// Creates a new task service.
    ///
    /// # Errors
    ///
    /// Returns error if we can't spawn the background thread.
    pub fn new() -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name("task-service".into())
            .spawn(move || {
                Self::run(receiver);
            })?;

        Ok(Self {
            active: Arc::new(AtomicBool::new(true)),
            sender,
        })
    }

    #[tracing::instrument(skip(receiver))]
    fn run(receiver: mpsc::Receiver<Command>) {
        debug!("Starting task service");

        let mut wakers: HashMap<usize, Waker> = HashMap::new();
        let mut paused = false;
        let mut num_futures = 0_usize;
        let mut pause_awaiters = Vec::new();

        let notify_awaiters =
            |paused_count: usize,
             future_count: usize,
             pause_awaiters: &mut Vec<oneshot::Sender<()>>| {
                if paused_count == future_count {
                    trace!("All futures paused");

                    for sender in pause_awaiters.drain(..) {
                        let _ = sender.send(());
                    }
                }
            };

        while let Ok(command) = receiver.recv() {
            match command {
                Command::Pause(sender) => {
                    paused = true;

                    if let Some(sender) = sender {
                        pause_awaiters.push(sender);
                    }
                }

                Command::Resume => {
                    paused = false;

                    for (id, waker) in wakers.drain() {
                        trace!("Waking future {}", id);
                        waker.wake();
                    }
                }

                Command::FutureCreated { id } => {
                    trace!("Future {} created", id);

                    num_futures = num_futures.saturating_add(1);
                }

                Command::FuturePaused { id, waker } => {
                    if !paused {
                        trace!("Waking future {}", id);
                        waker.wake();
                        continue;
                    }

                    wakers.insert(id, waker);

                    notify_awaiters(wakers.len(), num_futures, &mut pause_awaiters);
                }

                Command::FutureDropped { id } => {
                    trace!("Future {} dropped", id);

                    wakers.remove(&id);
                    num_futures = num_futures.saturating_sub(1);

                    notify_awaiters(wakers.len(), num_futures, &mut pause_awaiters);
                }
            }
        }

        debug!("Stopping task service");
    }

    /// Pauses running (and future) tasks until you call [`Self::resume()`].
    ///
    /// Futures are paused on their next `.poll()` cycle, i.e. they are not
    /// interrupted immediately (which we couldn't do anyway even if we wanted
    /// to).
    pub fn pause(&self) {
        info!("Pausing tasks");

        self.active.store(false, Ordering::Relaxed);

        if let Err(e) = self.sender.send(Command::Pause(None)) {
            error!("Failed to send pause command: {}", e);
        }
    }

    /// Like [`Self::pause()`], but instead of returning immediately, it waits
    /// for all futures to actually pause.
    ///
    /// # Errors
    ///
    /// Returns error if the task service's thread has crashed and is unable to
    /// service the request.
    pub async fn pause_and_wait(&self) -> Result<(), oneshot::error::RecvError> {
        info!("Pausing tasks and await");

        self.active.store(false, Ordering::Relaxed);

        let (sender, receiver) = oneshot::channel();

        if let Err(e) = self.sender.send(Command::Pause(Some(sender))) {
            error!("Failed to send pause command: {}", e);
        }

        receiver.await
    }

    /// Resumes all paused tasks.
    pub fn resume(&self) {
        info!("Resuming tasks");

        self.active.store(true, Ordering::Relaxed);

        if let Err(e) = self.sender.send(Command::Resume) {
            error!("Failed to send resume command: {}", e);
        }
    }

    /// Spawns a new task.
    ///
    /// The spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.spawn_with::<DefaultTaskSpawner, _>(future)
    }

    /// Like [`Self::spawn()`], but using given [`TaskSpawner`].
    pub fn spawn_with<S, F>(&self, future: F) -> JoinHandle<F::Output>
    where
        S: TaskSpawner,
        F: Future<Output: Send> + Send + 'static,
    {
        S::spawn(self.guard(future))
    }

    pub(crate) fn guard<F>(&self, future: F) -> Pin<Box<GuardedFuture<F>>>
    where
        F: Future,
    {
        let guard = FutureGuard {
            id: 0,
            drain: Cell::new(false),
            active: self.active.clone(),
            parent: self.sender.clone(),
        };

        let mut future = Box::new(GuardedFuture::new(future, guard));

        // Pointers to live objects are unique by definition, so it's a
        // convenient way of uniquely identifying alive Futures:
        future.guard.id = (&raw const *future) as usize;

        let _ = self.sender.send(Command::FutureCreated {
            id: future.guard.id,
        });

        Pin::from(future)
    }
}

#[pin_project]
pub(crate) struct GuardedFuture<F: Future> {
    #[pin]
    future: F,
    guard: FutureGuard,
}

impl<F: Future> GuardedFuture<F> {
    fn new(future: F, guard: FutureGuard) -> Self {
        Self { future, guard }
    }
}

impl<F: Future> Future for GuardedFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.guard.can_continue(cx) {
            return Poll::Pending;
        }

        DRAIN.set(false);

        let poll = self.as_mut().project().future.poll(cx);

        // Intuitively, what we need to discover is:
        //
        // > Is there any `NonPausableFuture` that we've reached that has
        // > yielded `Poll::Pending`? If so, we'll have to poll this future
        // > again the next time, until it returns `Poll::Ready(_)`.
        //
        // In some languages this can be implemented using algebraic effects -
        // we can imagine `NonPausableFuture` throwing a kind of "SuperPending"
        // effect - but in our case a thread-local will do.
        self.guard.drain.set(DRAIN.get());

        poll
    }
}

struct FutureGuard {
    id: usize,
    drain: Cell<bool>,
    active: Arc<AtomicBool>,
    parent: mpsc::Sender<Command>,
}

impl FutureGuard {
    fn can_continue(&self, cx: &mut Context<'_>) -> bool {
        if self.drain.get() || self.active.load(Ordering::Relaxed) {
            return true;
        }

        if self
            .parent
            .send(Command::FuturePaused {
                id: self.id,
                waker: cx.waker().clone(),
            })
            .is_err()
        {
            // Task service must've died - if we don't continue, we'll turn into
            // a zombie
            return true;
        }

        trace!("Pausing future {}", self.id);

        false
    }
}

impl Drop for FutureGuard {
    fn drop(&mut self) {
        let _ = self.parent.send(Command::FutureDropped { id: self.id });
    }
}

/// Future that, once polled, can't be paused.
///
/// Wrapping a [`Future`] with this causes that future to get polled even if
/// someone calls [`TaskService::pause()`].
///
/// Note that this doesn't guarantee polling into completion, since your caller
/// can always just `mem::forget()` you - this only affects the task service's
/// pausing mechanism.
///
/// If task service is already paused when you're spawning this future, it will
/// be spawned as paused as well, i.e. the future must've gotten polled at least
/// once for the non-pausing mechanism to kick in.
///
/// See: [`IntoNonPausableFuture`].
#[pin_project]
pub struct NonPausableFuture<F: Future> {
    #[pin]
    future: F,
}

impl<F: Future> NonPausableFuture<F> {
    fn new(future: F) -> Self {
        Self { future }
    }
}

impl<F: Future> Future for NonPausableFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().future.poll(cx) {
            Poll::Ready(value) => Poll::Ready(value),

            Poll::Pending => {
                // Notify our parent, `GuardedFuture`, that we're still pending.
                //
                // This could be also done via `cx.ext()`, but no need to go
                // that crazy (plus it's still a nightly feature).
                _ = DRAIN.try_with(|drain| {
                    drain.set(true);
                });

                Poll::Pending
            }
        }
    }
}

pub trait IntoNonPausableFuture: Future + Sized {
    /// Converts this future into a future that can't be paused.
    ///
    /// See: [`NonPausableFuture`].
    fn into_non_pausable(self) -> NonPausableFuture<Self> {
        NonPausableFuture::new(self)
    }
}

impl<T: Future> IntoNonPausableFuture for T {
    //
}

#[derive(Debug)]
enum Command {
    Pause(Option<oneshot::Sender<()>>),
    Resume,

    FutureCreated { id: usize },
    FuturePaused { id: usize, waker: Waker },
    FutureDropped { id: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::{sync::RwLock, task, time};
    use tracing::{Instrument, trace_span};

    #[tokio::test(flavor = "multi_thread")]
    async fn overwrite_pause_state() {
        let service = TaskService::new().unwrap();
        let (sender1, mut receiver1) = tokio::sync::mpsc::channel(1);
        let (sender2, mut receiver2) = tokio::sync::mpsc::channel(1);
        let (main_sender, mut main_receiver) = tokio::sync::mpsc::channel(2);

        let lock = Arc::new(RwLock::new(0));
        let guard = lock.write().await;
        let lock_clone = Arc::clone(&lock);
        let main_sender_clone = main_sender.clone();

        service.spawn(async move {
            // Notify main task that we are ready.
            main_sender_clone.send(()).await.unwrap();

            // Wait to acquire read lock, this means the service should be paused now.
            let guard = lock_clone.read().await;
            drop(guard);

            // this future only completes after we resume execution
            sender1.send(()).await.unwrap();
        });

        let lock_clone = Arc::clone(&lock);
        let main_sender_clone = main_sender.clone();

        service.spawn(
            async move {
                // Notify main task that we are ready.
                main_sender_clone.send(()).await.unwrap();
                // Wait to acquire read lock, this means the service should be paused now.
                let guard = lock_clone.read().await;
                drop(guard);
                // Completes the future immediately
                sender2.send(()).await.unwrap();
            }
            .into_non_pausable(),
        );

        // wait on futures to be ready
        main_receiver.recv().await.unwrap();
        main_receiver.recv().await.unwrap();

        // pause services
        service.pause();

        // unblock futures
        drop(guard);

        // Wait on answer from unpausable future
        time::timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .unwrap();

        // Normal future will not trigger anything
        time::timeout(Duration::from_millis(100), receiver1.recv())
            .await
            .unwrap_err();

        // Resume service
        service.resume();

        // Normal future now completes
        receiver1.recv().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[tracing_test::traced_test]
    async fn pause_and_wait() {
        let service = TaskService::new().unwrap();
        let (sender1, mut receiver1) = tokio::sync::mpsc::channel(1);
        let (sender2, mut receiver2) = tokio::sync::mpsc::channel(1);
        let (main_sender, mut main_receiver) = tokio::sync::mpsc::channel(2);

        let lock = Arc::new(RwLock::new(0));
        let guard = lock.write().await;
        let lock_clone = Arc::clone(&lock);
        let main_sender_clone = main_sender.clone();

        service.spawn(
            async move {
                // Notify main task that we are ready.
                main_sender_clone.send(()).await.unwrap();
                // Wait to acquire read lock, this means the service should be paused now.
                let guard = lock_clone.read().await;
                drop(guard);
                info!("Sleeping");
                time::sleep(Duration::from_millis(500)).await;
                // this future only completes after we resume execution
                sender1.send(()).await.unwrap();
                info!("DONE");
            }
            .instrument(trace_span!("FUTURE1")),
        );

        let lock_clone = Arc::clone(&lock);
        let main_sender_clone = main_sender.clone();

        service.spawn(
            async move {
                async move {
                    // Notify main task that we are ready.
                    main_sender_clone.send(()).await.unwrap();
                    // Wait to acquire read lock, this means the service should be paused now.
                    let guard = lock_clone.read().await;
                    drop(guard);
                    info!("Sleeping");
                    time::sleep(Duration::from_millis(500)).await;
                    info!("Done");
                }
                .instrument(trace_span!("NONPAUSE"))
                .into_non_pausable()
                .await;
                info!("Sleeping");
                time::sleep(Duration::from_millis(500)).await;
                sender2.send(()).await.unwrap();
                info!("Done");
            }
            .instrument(trace_span!("FUTURE2")),
        );

        let lock_clone = Arc::clone(&lock);
        let main_sender_clone = main_sender.clone();

        // this future will be aborted
        let join_handle = service.spawn(
            async move {
                // Notify main task that we are ready.
                main_sender_clone.send(()).await.unwrap();
                // Wait to acquire read lock, this means the service should be paused now.
                let guard = lock_clone.read().await;
                drop(guard);
                time::sleep(Duration::from_millis(5)).await;
            }
            .instrument(trace_span!("FUTURE3")),
        );

        // wait on futures to be ready
        main_receiver.recv().await.unwrap();
        main_receiver.recv().await.unwrap();
        main_receiver.recv().await.unwrap();
        service.pause();

        // unblock futures
        drop(guard);

        // Abort future.
        join_handle.abort();

        // re-pause and await
        time::timeout(Duration::from_secs(2), service.pause_and_wait())
            .await
            .unwrap()
            .unwrap();

        // Ensure future remains paused.
        time::timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .unwrap_err();

        // Ensure future remains paused.
        time::timeout(Duration::from_millis(100), receiver1.recv())
            .await
            .unwrap_err();

        // Resume service
        service.resume();

        // Wait on future completion
        time::timeout(Duration::from_millis(600), receiver1.recv())
            .await
            .unwrap();

        time::timeout(Duration::from_millis(600), receiver2.recv())
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[tracing_test::traced_test]
    async fn non_pausable_with_multiple_yield_points() {
        let service = Arc::new(TaskService::new().unwrap());

        let value = service.spawn({
            let service = service.clone();

            async move {
                service.pause();

                task::yield_now().await;
                task::yield_now().await;
                task::yield_now().await;
            }
            .into_non_pausable()
        });

        time::timeout(Duration::from_millis(100), value)
            .await
            .unwrap()
            .unwrap();
    }
}
