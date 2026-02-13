use anyhow::anyhow;
use parking_lot::Mutex;
use pin_project::pin_project;
use std::cell::Cell;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use std::{io, thread};
use tokio::runtime;
use tokio::sync::oneshot;
use tokio::task::{AbortHandle, JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace};

thread_local! {
    static DRAIN: Cell<bool> = const { Cell::new(false) };
}

/// Manages futures that can be paused on demand.
pub struct TaskService {
    active: Arc<AtomicBool>,
    sender: mpsc::Sender<Command>,
    runtime: runtime::Handle,
}

impl TaskService {
    /// Creates a new task service.
    pub fn new(runtime: runtime::Handle) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name("task-service".into())
            .spawn(move || {
                Self::run(&receiver);
            })?;

        Ok(Self {
            active: Arc::new(AtomicBool::new(true)),
            sender,
            runtime,
        })
    }

    #[tracing::instrument(skip_all)]
    fn run(receiver: &mpsc::Receiver<Command>) {
        info!("Starting task service");

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

                    notify_awaiters(wakers.len(), num_futures, &mut pause_awaiters);
                }

                Command::Resume => {
                    paused = false;

                    // If we receive a resume after pause and await, we need to unblock
                    // all the waiters or they will wait forever.
                    for sender in pause_awaiters.drain(..) {
                        let _ = sender.send(());
                    }

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

        info!("Stopping task service");
    }

    /// Pauses the currently running (and possible future) tasks until you call
    /// [`Self::resume()`].
    ///
    /// Futures are paused as soon as they return from the call to `.poll()`.
    pub fn pause(&self) {
        info!("Pausing tasks");

        self.active.store(false, Ordering::Relaxed);

        if let Err(e) = self.sender.send(Command::Pause(None)) {
            error!("Failed to send pause command: {e:?}");
        }
    }

    /// Like [`Self::pause()`], but instead of returning immediately, it waits
    /// for all of the futures to be actually paused.
    ///
    /// # Remarks
    ///
    /// If the spawned futures are awaiting on different await points they may not report
    /// the fact that they are paused. It is possible that this function never recovers. A
    /// `timeout` is required to avoid "surprise" blocked forever.
    pub async fn pause_and_wait(&self, timeout: Duration) -> anyhow::Result<()> {
        info!("Pausing tasks and waiting");

        self.active.store(false, Ordering::Relaxed);

        let (sender, receiver) = oneshot::channel();

        if let Err(e) = self.sender.send(Command::Pause(Some(sender))) {
            error!("Failed to send pause command: {}", e);
        }

        // This is a failsafe mechanism.
        // This should never timeout, that means that we have a bug here.
        // However, we've had bugs here which tend to crash the whole application,
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(anyhow!("The sender dropped!")),
            Err(_) => Err(anyhow!(
                "Pausing for non-pausable futures failed. Some futures are paused in other locations."
            )),
        }
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
    /// Spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.runtime.spawn(self.guard(future))
    }

    /// Spawns a new task that races with given cancellation token.
    ///
    /// Spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn_cancellable<F>(&self, token: CancellationToken, future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel::<AbortHandle>();

        let handle = self.spawn(async move {
            if let Some(value) = token.run_until_cancelled(future).await {
                value
            } else {
                // If `token` got cancelled, abort our task
                rx.await.unwrap().abort();

                // Soft-unreachable - once the called awaits the `JoinHandle`,
                // it will return a `JoinError(Cancelled)`
                std::future::pending().await
            }
        });

        _ = tx.send(handle.abort_handle());

        handle
    }

    fn guard<F>(&self, future: F) -> Pin<Box<GuardedFuture<F>>>
    where
        F: Future,
    {
        let guard = FutureGuard {
            id: 0,
            drain: false,
            active: self.active.clone(),
            service: self.sender.clone(),
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ServiceState {
    Active,
    Suspended,
}

impl ServiceState {
    fn is_active(self) -> bool {
        self == ServiceState::Active
    }

    fn is_suspended(self) -> bool {
        self == ServiceState::Suspended
    }
}

struct BackgroundAwareTaskState {
    main: ServiceState,
    background: ServiceState,
    background_tasks: usize,
}

impl BackgroundAwareTaskState {
    fn new() -> Self {
        Self {
            main: ServiceState::Active,
            background: ServiceState::Suspended,
            background_tasks: 0,
        }
    }

    fn pause_main(&mut self) -> bool {
        // Only pause if main is running and background is suspended.
        let should_pause = self.main.is_active() && self.background.is_suspended();
        self.main = ServiceState::Suspended;
        should_pause
    }

    fn pause_background(&mut self) -> bool {
        // Only pause if background is running and main is suspended, and we are the last
        // background request which initiated this request.
        let do_pause =
            self.main.is_suspended() && self.background.is_active() && self.background_tasks == 1;
        self.background_tasks = self.background_tasks.saturating_sub(1);
        if self.background_tasks == 0 {
            self.background = ServiceState::Suspended;
        }
        do_pause
    }

    fn resume_background(&mut self) -> bool {
        // Only resume main if both main and background are suspended, and we are the first
        // background task to initiate this request.
        let do_resume = self.main.is_suspended()
            && self.background.is_suspended()
            && self.background_tasks == 0;
        self.background_tasks = self.background_tasks.saturating_add(1);
        self.background = ServiceState::Active;
        do_resume
    }

    fn resume_main(&mut self) -> bool {
        // Only resume main if both main and background are suspended.
        let do_resume = self.main.is_suspended() && self.background.is_suspended();
        self.main = ServiceState::Active;
        do_resume
    }
}

/// Similar to [`TaskService`] but provides more control when the spawned tasked should be paused
/// and resumed when used in a background job on the platform in the same OS process.
///
/// To achieve this we track 2 states: one for the main application and one for the background task
/// When the main application goes into a suspended state, [`pause_main()`] should be called and
/// [`resume_main()`] when it enters the foreground. When the background tasks(s) start, they should
/// call [`resume_background()`] and [`pause_background()`].
///
/// Internally we will ensure the underlying [`TaskService`] is paused only when required, allowing
/// it to be safely shared between various background tasks and the main application.
pub struct BackgroundAwareTaskService {
    service: Arc<TaskService>,
    state: Mutex<BackgroundAwareTaskState>,
}

impl BackgroundAwareTaskService {
    /// Create a new instance where the main states is active and the background suspended.
    #[must_use]
    pub fn new(service: TaskService) -> Self {
        Self {
            state: Mutex::new(BackgroundAwareTaskState::new()),
            service: Arc::new(service),
        }
    }

    /// Pause tasks when the main application is about to go into a suspended state. If a background
    /// task is running, the pause request will be ignored.
    pub fn pause_main(&self) {
        info!("Request to pause work from main thread");
        if self.state.lock().pause_main() {
            self.service.pause();
        }
    }

    /// Pause tasks and wait for all task to be paused when the main application is about to go
    /// into a suspended state. If a background task is running, the pause request will be ignored
    /// and we will return immediately.
    pub async fn pause_main_and_wait(&self, timeout: Duration) -> anyhow::Result<()> {
        info!("Request to pause work and wait from main thread");
        if self.state.lock().pause_main() {
            self.service.pause_and_wait(timeout).await
        } else {
            Ok(())
        }
    }

    /// Pause tasks when the background task has finished running. If the main application is not
    /// in a suspended state, teh request will be ignored an we will return immediately.
    pub fn pause_background(&self) {
        info!("Request to pause work from background thread");
        if self.state.lock().pause_background() {
            self.service.pause();
        }
    }

    /// Pause tasks and wait for all task to be paused when the background task has finished running.
    /// If the main application is not in a suspended state, teh request will be ignored.
    pub async fn pause_background_and_wait(&self, timeout: Duration) -> anyhow::Result<()> {
        info!("Request to pause work and wait from background thread");
        if self.state.lock().pause_background() {
            self.service.pause_and_wait(timeout).await
        } else {
            Ok(())
        }
    }

    /// Resume all tasks if the main application and the background task are not suspended. In all
    /// other combinations, the request will be ignored.
    pub fn resume_main(&self) {
        info!("Request to resume work from main thread");
        let mut state = self.state.lock();
        if state.resume_main() {
            self.service.resume();
        }
    }

    /// Resume all tasks if the main application and the background task are not suspended. In all
    /// other combinations, the request will be ignored.
    pub fn resume_background(&self) {
        info!("Request to resume work from background thread");
        let mut state = self.state.lock();
        if state.resume_background() {
            self.service.resume();
        }
    }

    /// Resumes and pauses the background task creating a "scoped" function.
    pub fn scope_background<O>(&self, f: impl FnOnce() -> O) -> O {
        self.resume_background();
        let out = f();
        self.pause_background();
        out
    }

    /// Resumes and pauses the background task creating a "scoped" function.
    pub async fn scope_background_async<O>(&self, f: impl AsyncFnOnce() -> O) -> O {
        self.resume_background();
        let out = f().await;
        self.pause_background();
        out
    }

    /// Spawns a new task.
    ///
    /// Spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.service.spawn(future)
    }

    /// Spawns a new task that races with given cancellation token.
    ///
    /// Spawned task can have its execution paused with [`Self::pause()`].
    pub fn spawn_cancellable<F>(&self, token: CancellationToken, future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.service.spawn_cancellable(token, future)
    }

    /// Get a reference to the underlying `TaskService`.
    ///
    /// This method is provided for bwd compat. For new code that needs
    /// to share the `TaskService` instance, use [`task_service_arc()`](Self::task_service_arc) instead.
    pub fn task_service(&self) -> &TaskService {
        &self.service
    }

    /// Get an `Arc` reference to the underlying `TaskService`.
    ///
    /// This allows sharing the same `TaskService` instance across multiple compoents
    /// without creating duplicate instances.
    pub fn task_service_arc(&self) -> Arc<TaskService> {
        self.service.clone()
    }
}

/// Manages the inner-future, deciding whether it can be polled or not.
#[pin_project]
struct GuardedFuture<F: Future> {
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if !this.guard.can_continue(cx) {
            return Poll::Pending;
        }

        DRAIN.set(false);

        let poll = this.future.poll(cx);

        // Intuitively, what we need to discover is:
        //
        // > Is there any `NonPausableFuture` that we've reached that has
        // > yielded `Poll::Pending`? If so, we'll have to poll this future
        // > again the next time, until it returns `Poll::Ready(_)`.
        //
        // In some languages this can be implemented using algebraic effects -
        // we can imagine `NonPausableFuture` throwing a kind of "SuperPending"
        // effect - but in our case a thread-local will do.
        this.guard.drain = DRAIN.get();

        poll
    }
}

struct FutureGuard {
    id: usize,
    drain: bool,
    active: Arc<AtomicBool>,
    service: mpsc::Sender<Command>,
}

impl FutureGuard {
    fn can_continue(&self, cx: &mut Context<'_>) -> bool {
        if self.drain || self.active.load(Ordering::Relaxed) {
            return true;
        }

        if self
            .service
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
        let _ = self.service.send(Command::FutureDropped { id: self.id });
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
/// be spawned as paused as well, i.e. the future must be polled at least once
/// for the non-pausing mechanism to kick in.
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

    fn service() -> TaskService {
        TaskService::new(runtime::Handle::current()).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn overwrite_pause_state() {
        let service = service();
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
        let service = service();
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
        service
            .pause_and_wait(Duration::from_secs(2))
            .await
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
        let service = Arc::new(service());

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

    #[tokio::test(flavor = "multi_thread")]
    #[tracing_test::traced_test]
    async fn pause_and_wait_does_not_bock() {
        let service = Arc::new(service());

        service
            .pause_and_wait(Duration::from_millis(100))
            .await
            .unwrap();
    }

    #[test]
    fn background_state_transitions_main_active_background_paused() {
        let mut state = BackgroundAwareTaskState::new();
        // pause main
        assert!(state.pause_main());
        // pause resume main
        assert!(state.resume_main());
    }

    #[test]
    fn background_state_transitions_main_active_background_active() {
        let mut state = BackgroundAwareTaskState::new();
        // start background should be noop since main is active
        assert!(!state.resume_background());
        assert!(!state.pause_background());
    }

    #[test]
    fn background_state_transitions_main_paused_background_active() {
        let mut state = BackgroundAwareTaskState::new();
        assert!(state.pause_main());
        assert!(state.resume_background());
        assert!(state.pause_background());
    }

    #[test]
    fn background_state_transitions_main_paused_background_active_then_main_active_and_background_paused()
     {
        let mut state = BackgroundAwareTaskState::new();
        assert!(state.pause_main());
        assert!(state.resume_background());
        assert!(!state.resume_main());
        assert!(!state.pause_background());
    }

    #[test]
    fn background_state_transitions_main_paused_background_active_then_background_pause_and_main_active()
     {
        let mut state = BackgroundAwareTaskState::new();
        assert!(state.pause_main());
        assert!(state.resume_background());
        assert!(state.pause_background());
        assert!(state.resume_main());
    }

    #[test]
    fn background_state_transitions_main_paused_multiple_background_tasks() {
        let mut state = BackgroundAwareTaskState::new();
        assert!(state.pause_main());
        // 1st task resumes background work.
        assert!(state.resume_background());
        // 2nd task resumes background work - noop
        assert!(!state.resume_background());
        // 2nd task finishes background work - noop
        assert!(!state.pause_background());
        // 1st task finished background work - we can pause
        assert!(state.pause_background());
    }
}
