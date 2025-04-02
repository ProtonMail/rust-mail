use crate::TaskSpawner;
use crate::spawn::DefaultTaskSpawner;
use pin_project::pin_project;
use std::cell::Cell;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::task::{Context, Poll, Waker};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

thread_local! {
    static DRAIN: Cell<bool> = const { Cell::new(false) };
}

/// Trait to convert any future into a pausable future.
pub trait IntoPausableFuture: Future + Sized + Send + 'static {
    /// Convert the current future into a pausable future controled by the given `service`.
    fn into_pausable(
        self,
        service: &TaskService,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        service.wrap_future(self)
    }
}

impl<T: Future + Sized + Send + 'static> IntoPausableFuture for T {}

/// Future Wrapper that will pause execution when instructed by a [`TaskService`].
#[pin_project]
struct PausableFuture<F: Future> {
    state: FutureState,
    #[pin]
    future: F,
}

impl<F: Future> PausableFuture<F> {
    fn new(future_state: FutureState, future: F) -> Self {
        Self {
            state: future_state,
            future,
        }
    }
}
impl<F: Future> Future for PausableFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.state.allowed_to_progress(cx) {
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
        self.state.drain.set(DRAIN.get());

        poll
    }
}

/// A future that can not be paused by the [`TaskService`].
///
/// This should be used by futures which should not have their execution interrupted. Prime
/// candidates includes operations that hold onto file locks such as sqlite database transactions.
///
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

/// Trait to convert any future into a future that can not be paused.
pub trait IntoNonPausableFuture: Future + Sized {
    fn into_non_pausable(self) -> NonPausableFuture<Self> {
        NonPausableFuture::new(self)
    }
}

impl<T: Future> IntoNonPausableFuture for T {}

/// Provides a service that spawns futures which may be paused and resumed.
pub struct TaskService {
    id_allocator: AtomicU64,
    allowed_to_run: Arc<AtomicBool>,
    sender: Sender<Command>,
}

impl TaskService {
    /// Create a new task service.
    ///
    /// # Errors
    ///
    /// Returns error if we can't name the background thread.
    pub fn new() -> std::io::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("task-service".into())
            .spawn(move || {
                Self::run(&receiver);
            })?;

        Ok(Self {
            id_allocator: AtomicU64::new(0),
            allowed_to_run: Arc::new(AtomicBool::new(true)),
            sender,
        })
    }

    fn run(receiver: &Receiver<Command>) {
        debug!("Starting task service");
        let mut watchers: HashMap<u64, Waker> = HashMap::new();
        let mut paused = false;
        let mut num_futures = 0_usize;
        let mut pause_awaiters = Vec::<oneshot::Sender<()>>::new();

        let notify_awaiters =
            |paused_count: usize,
             future_count: usize,
             pause_awaiters: &mut Vec<oneshot::Sender<()>>| {
                if paused_count == future_count {
                    tracing::trace!("All futures paused");
                    for sender in pause_awaiters.drain(..) {
                        let _ = sender.send(());
                    }
                }
            };
        while let Ok(command) = receiver.recv() {
            match command {
                Command::Pause => {
                    paused = true;
                }
                Command::PauseAndWait(sender) => {
                    paused = true;
                    pause_awaiters.push(sender);
                }
                Command::Resume => {
                    paused = false;
                    // Wake up all the paused futures.
                    for (id, waker) in watchers.drain() {
                        tracing::trace!("Waking future {}", id);
                        waker.wake();
                    }
                }
                Command::Queue { id, waker } => {
                    // If we are not paused anymore, but still receive a waker registration
                    // immediately waken the waker.
                    if !paused {
                        tracing::trace!("Waking future {}", id);
                        waker.wake();
                        continue;
                    }

                    // Register watcher and await the unpause command.
                    watchers.insert(id, waker);

                    // Notify all paused waiters that all the futures are paused now.
                    notify_awaiters(watchers.len(), num_futures, &mut pause_awaiters);
                }
                Command::Dropped { id } => {
                    tracing::trace!("Future dropped: {}", id);
                    watchers.remove(&id);
                    num_futures = num_futures.saturating_sub(1);
                    // Notify all paused waiters that all the futures are paused now.
                    notify_awaiters(watchers.len(), num_futures, &mut pause_awaiters);
                }
                Command::Started { id } => {
                    tracing::trace!("Future started: {}", id);
                    num_futures = num_futures.saturating_add(1);
                }
            }
        }
        debug!("Starting task service terminating");
    }

    /// Pause all running async task.
    ///
    /// # Remarks
    ///
    /// This will eventually cause the futures spawned via this to pause on their next poll cycle.
    pub fn pause(&self) {
        info!("Pausing tasks");
        self.allowed_to_run.store(false, Ordering::Relaxed);
        if let Err(e) = self.sender.send(Command::Pause) {
            error!("Failed to send pause command: {}", e);
        }
    }

    /// Pause all running async task.
    ///
    /// # Remarks
    ///
    /// This will eventually cause the futures spawned via this to pause on their next poll cycle.
    ///
    /// As soon as all the spawned futures have entered the paused state, this future will resolve.
    /// If after this method new tasks are spawned, it is possible that this method will return
    /// early if all current futures have paused.
    ///
    /// # Errors
    ///
    /// Returns error if we did not receive a reply.
    pub async fn pause_and_wait(&self) -> Result<(), oneshot::error::RecvError> {
        info!("Pausing tasks and await");
        self.allowed_to_run.store(false, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self.sender.send(Command::PauseAndWait(sender)) {
            error!("Failed to send pause command: {}", e);
            drop(e);
        }
        receiver.await
    }

    /// Unpause all paused async tasks.
    pub fn resume(&self) {
        info!("Resuming tasks");
        self.allowed_to_run.store(true, Ordering::Relaxed);
        if let Err(e) = self.sender.send(Command::Resume) {
            error!("Failed to send resume command: {}", e);
        }
    }

    /// Spawn a new task using the [`DefaultTaskSpawner`].
    ///
    /// The spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send,
    {
        self.spawn_with::<DefaultTaskSpawner, _>(future)
    }

    /// Spawn a new task using the given [`TaskSpawner`].
    ///
    /// The spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn_with<S, F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send,
        S: TaskSpawner,
    {
        S::spawn(self.wrap_future(future))
    }

    fn wrap_future<F: Future + Send + 'static>(
        &self,
        future: F,
    ) -> impl Future<Output = F::Output> + Send + 'static {
        // Each future needs an unique id, as it is possible the waker can change
        // if the future is re-scheduled to a different execution thread.
        let future_state = FutureState {
            id: self.id_allocator.fetch_add(1, Ordering::AcqRel),
            drain: Cell::new(false),
            allowed_to_run: self.allowed_to_run.clone(),
            channel: self.sender.clone(),
        };

        // Communicate new future
        let _ = self.sender.send(Command::Started {
            id: future_state.id,
        });

        PausableFuture::new(future_state, future)
    }
}

/// Background worker command
enum Command {
    /// Signal that a future has started
    Started { id: u64 },
    /// Signal that we entered the paused state.
    Pause,
    /// Signal a request to pause all the futures and wait until they are all paused.
    PauseAndWait(oneshot::Sender<()>),
    /// Signal that we exited the paused state.
    Resume,
    /// Register a waker for a future.
    Queue { id: u64, waker: Waker },
    /// Cleanup future waker on drop
    Dropped { id: u64 },
}

tokio::task_local! {
    static OVERWRITE_PAUSE: Cell<bool>;
}

/// Future state to check whether we are paused or not.
struct FutureState {
    id: u64,
    drain: Cell<bool>,
    allowed_to_run: Arc<AtomicBool>,
    channel: Sender<Command>,
}

impl Drop for FutureState {
    fn drop(&mut self) {
        let _ = self.channel.send(Command::Dropped { id: self.id });
    }
}

impl FutureState {
    /// Check whether we are allowed to progress.
    fn allowed_to_progress(&self, cx: &mut Context<'_>) -> bool {
        if self.drain.get() || self.allowed_to_run.load(Ordering::Relaxed) {
            return true;
        }
        // Register waker
        if self
            .channel
            .send(Command::Queue {
                id: self.id,
                waker: cx.waker().clone(),
            })
            .is_err()
        {
            // Background thread must be dead so we can't pause or we will turn
            // into zombie.
            return true;
        }

        tracing::trace!("Pausing future {}", self.id);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::RwLock;
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
        tokio::time::timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .unwrap();
        // Normal future will not trigger anything
        tokio::time::timeout(Duration::from_millis(100), receiver1.recv())
            .await
            .unwrap_err();
        // Resume service
        service.resume();
        // Normal future now completes
        receiver1.recv().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[tracing_test::traced_test]
    async fn pause_and_await() {
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
                tracing::info!("Sleeping");
                tokio::time::sleep(Duration::from_millis(500)).await;
                // this future only completes after we resume execution
                sender1.send(()).await.unwrap();
                tracing::info!("DONE");
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
                    tracing::info!("Sleeping");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    tracing::info!("Done");
                }
                .instrument(trace_span!("NONPAUSE"))
                .into_non_pausable()
                .await;
                tracing::info!("Sleeping");
                tokio::time::sleep(Duration::from_millis(500)).await;
                sender2.send(()).await.unwrap();
                tracing::info!("Done");
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
                tokio::time::sleep(Duration::from_millis(5)).await;
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
        tokio::time::timeout(Duration::from_secs(2), service.pause_and_wait())
            .await
            .unwrap()
            .unwrap();

        // Ensure future remains paused.
        tokio::time::timeout(Duration::from_millis(100), receiver2.recv())
            .await
            .unwrap_err();
        // Ensure future remains paused.
        tokio::time::timeout(Duration::from_millis(100), receiver1.recv())
            .await
            .unwrap_err();
        // Resume service
        service.resume();
        // Wait on future completion
        tokio::time::timeout(Duration::from_millis(600), receiver1.recv())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(600), receiver2.recv())
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

                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
            }
            .into_non_pausable()
        });

        tokio::time::timeout(Duration::from_millis(100), value)
            .await
            .unwrap()
            .unwrap();
    }
}
