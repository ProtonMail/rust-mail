use crate::foreground_loop::EventLoop;
use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::Subscriber;
use crate::{Event, EventLoopError};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::spawn;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Response returned by the `LoopErrorHandler` to control the behavior of the event loop after an error occurs.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EventLoopErrorHandlerReply {
    /// Pause the loop execution until it is manually resumed.
    Pause,
    /// Retry the event loop with the same event id.
    Retry,
    /// Abort and stop processing all events.
    Abort,
}

/// If the event loop runs into an error, the user can control the desired behavior through an implementation of
/// this trait.
#[cfg_attr(test, mockall::automock)]
pub trait EventLoopErrorHandler: Send + Sync {
    fn on_error(&self, error: EventLoopError) -> EventLoopErrorHandlerReply;
}

/// This type polls the proton events at a given interval and distributes incoming events among its subscribers.
#[derive(Clone)]
pub struct BackgroundEventLoop<T: Event + From<<T as Event>::Response> + 'static> {
    inner: Arc<SharedBackgroundEventLoopState<T>>,
}
impl<T: Event + From<<T as Event>::Response> + 'static> Default for BackgroundEventLoop<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Event + From<<T as Event>::Response> + 'static> BackgroundEventLoop<T> {
    #[must_use]
    pub fn new() -> Self {
        let shared = Arc::new(SharedBackgroundEventLoopState {
            paused: AtomicBool::new(true),
            pending_subscribers: Mutex::new(Vec::new()),
            token: CancellationToken::new(),
        });

        Self { inner: shared }
    }

    /// Wait until the event loop has been cancelled. This does not imply the event loop has finished executing, but
    /// a cancel signal was received. The event loop will terminate shortly after that.
    pub async fn wait_on_cancelled(&self) {
        self.inner.token.cancelled().await;
    }

    /// Start the background task which will poll the event loop.
    ///
    /// # Errors
    /// Returns error if the background loop failed to initialize.
    pub async fn start(
        &self,
        interval: Duration,
        store: Box<dyn Store>,
        provider: Box<dyn Provider>,
        error_handler: Box<dyn EventLoopErrorHandler>,
    ) -> Result<JoinHandle<()>, EventLoopError> {
        let event_loop = EventLoop::new();

        event_loop
            .initialize(store.as_ref(), provider.as_ref())
            .await?;

        let mut loop_state = BackgroundLoopState {
            shared: self.inner.clone(),
            error_handler,
            event_loop,
            store,
            provider,
            subscribers: Vec::new(),
        };

        Ok(spawn(async move {
            loop_state.run(interval).await;
        }))
    }

    /// Cancel the execution of the event loop.
    pub fn cancel(&self) {
        self.inner.token.cancel();
    }

    /// Pause the event loop. Will affect the next poll cycle.
    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::Release);
    }

    /// Resume the event loop. Note that this is not an immediate action, the event loop will resume after the next
    /// interval timeout.
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::Release);
    }

    /// Check whether the event loop is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    /// Add a new subscriber to the event loop.
    pub fn subscribe(&self, subscriber: Box<dyn Subscriber<T>>) {
        let mut accessor = self.inner.pending_subscribers.lock();
        accessor.push(SubscriberOperation::Register(subscriber));
    }

    /// Remove a subscriber from the event loop.
    pub fn unsubscribe(&self, subscriber_name: impl Into<String>) {
        let mut accessor = self.inner.pending_subscribers.lock();
        accessor.push(SubscriberOperation::Unregister(subscriber_name.into()));
    }
}

impl<T: Event + From<<T as Event>::Response>> Drop for BackgroundEventLoop<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[doc(hidden)]
enum SubscriberOperation<T: Event> {
    Register(Box<dyn Subscriber<T>>),
    Unregister(String),
}

#[doc(hidden)]
struct SharedBackgroundEventLoopState<T: Event> {
    paused: AtomicBool,
    pending_subscribers: Mutex<Vec<SubscriberOperation<T>>>,
    token: CancellationToken,
}

#[doc(hidden)]
struct BackgroundLoopState<T: Event> {
    error_handler: Box<dyn EventLoopErrorHandler>,
    shared: Arc<SharedBackgroundEventLoopState<T>>,
    event_loop: EventLoop,
    store: Box<dyn Store>,
    subscribers: Vec<Box<dyn Subscriber<T>>>,
    provider: Box<dyn Provider>,
}

#[doc(hidden)]
impl<T: Event + From<<T as Event>::Response>> BackgroundLoopState<T> {
    /// This executes all [`SubscriberOperation`] batched every `poll_interval`.
    async fn run(&mut self, poll_interval: Duration) {
        let mut interval = interval(poll_interval);
        debug!("Starting background loop");
        while !self.shared.token.is_cancelled() {
            interval.tick().await;
            self.tick().await;
        }
    }

    /// Process pending subscriber operations
    async fn tick(&mut self) {
        // First process all register/unregister requests for subscribers.
        let accessor: Vec<_> = {
            // This should probably be a channel
            let mut guard = self.shared.pending_subscribers.lock();
            std::mem::take(&mut guard)
        };
        for operation in accessor {
            match operation {
                SubscriberOperation::Register(subscriber) => {
                    let new_subscriber_name = subscriber.name();
                    if !self
                        .subscribers
                        .iter()
                        .any(|v| v.name() == new_subscriber_name)
                    {
                        debug!("Registering subscriber {new_subscriber_name}");
                        self.subscribers.push(subscriber);
                    }
                }

                SubscriberOperation::Unregister(subscriber_name) => {
                    debug!("Unregistering subscriber {subscriber_name}");
                    self.subscribers.retain(|v| v.name() != subscriber_name);
                }
            }
        }

        if self.shared.paused.load(Ordering::Acquire) {
            return;
        }

        if let Err(e) = self
            .event_loop
            .poll(
                self.store.as_ref(),
                self.provider.as_ref(),
                &self.subscribers,
            )
            .await
        {
            self.on_error(e);
        }
    }

    fn on_error(&mut self, error: EventLoopError) {
        match self.error_handler.on_error(error) {
            EventLoopErrorHandlerReply::Pause => {
                self.shared.paused.store(true, Ordering::Release);
            }
            EventLoopErrorHandlerReply::Retry => {
                // Nothing to do
            }
            EventLoopErrorHandlerReply::Abort => {
                self.shared.token.cancel();
            }
        }
    }
}
