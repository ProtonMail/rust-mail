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
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.state.allowed_to_progress(cx) {
            return Poll::Pending;
        }
        self.project().future.poll(cx)
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
        // Disable the pause check.
        match OVERWRITE_PAUSE.try_with(|value| {
            if value.get() {
                false
            } else {
                value.set(true);
                true
            }
        }) {
            Ok(should_restore) => {
                match self.project().future.poll(cx) {
                    Poll::Ready(output) => {
                        // Restore the pause check when the future is finished, but only
                        // if it wasn't previously paused.
                        if should_restore {
                            OVERWRITE_PAUSE.with(|value| value.set(false));
                        }
                        Poll::Ready(output)
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            Err(_) => {
                // running outside of a pausable scope.
                self.project().future.poll(cx)
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
        while let Ok(command) = receiver.recv() {
            match command {
                Command::Pause => {
                    paused = true;
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
                }
                Command::Dropped { id } => {
                    tracing::trace!("Future dropped: {}", id);
                    watchers.remove(&id);
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
            allowed_to_run: self.allowed_to_run.clone(),
            channel: self.sender.clone(),
        };

        OVERWRITE_PAUSE.scope(Cell::new(false), PausableFuture::new(future_state, future))
    }
}

/// Background worker command
enum Command {
    /// Signal that we entered the paused state.
    Pause,
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
        // Check whether this a pause overwrite.
        if let Ok(value) = OVERWRITE_PAUSE.try_with(Cell::get) {
            if value {
                return true;
            }
        }
        // Check whether we are not in a paused state.
        if self.allowed_to_run.load(Ordering::Relaxed) {
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
}
