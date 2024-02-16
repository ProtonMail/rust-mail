use crate::provider::Provider;
use crate::store::Store;
use crate::{EventLoop, EventLoopError, Subscriber};
use proton_api_core::domain::IsEvent;
use proton_api_core::exports::tracing::debug;
use proton_async::futures::FutureExt;
use proton_async::util::CancellationToken;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
pub struct BackgroundEventLoop<T: IsEvent + 'static> {
    inner: Arc<SharedBackgroundEventLoopState<T>>,
}
impl<T: IsEvent + 'static> Default for BackgroundEventLoop<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: IsEvent + 'static> BackgroundEventLoop<T> {
    pub fn new() -> Self {
        let shared = Arc::new(SharedBackgroundEventLoopState {
            paused: AtomicBool::new(true),
            pending_subscribers: proton_async::sync::Mutex::new(Vec::new()),
            token: CancellationToken::new(),
        });

        Self { inner: shared }
    }

    /// Wait until the event loop has been cancelled. This does not imply the event loop has finished executing, but
    /// a cancel signal was received. The event loop will terminate shortly after that.
    pub async fn wait_on_cancelled(&self) {
        self.inner.token.cancelled().await
    }

    /// Start the background task which will poll the event loop.
    pub async fn start(
        &self,
        interval: Duration,
        store: Box<dyn Store>,
        provider: Box<dyn Provider<T>>,
        error_handler: Box<dyn EventLoopErrorHandler>,
    ) -> Result<impl proton_async::runtime::JoinHandle<()>, EventLoopError> {
        let event_loop = EventLoop::new(store, provider).await?;

        let mut loop_state = BackgroundLoopState {
            shared: self.inner.clone(),
            error_handler,
            event_loop,
        };

        Ok(proton_async::runtime::spawn(async move {
            loop_state.run(interval).await
        }))
    }

    /// Cancel the execution of the event loop.
    pub fn cancel(&self) {
        self.inner.token.cancel()
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
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    /// Add a new subscriber to the event loop.
    pub async fn subscribe(&self, subscriber: Box<dyn Subscriber<T>>) {
        let mut accessor = self.inner.pending_subscribers.lock().await;
        accessor.push(SubscriberOperation::Register(subscriber));
    }

    /// Remove a subscriber from the event loop.
    pub async fn unsubscribe(&self, subscriber_name: impl Into<String>) {
        let mut accessor = self.inner.pending_subscribers.lock().await;
        accessor.push(SubscriberOperation::Unregister(subscriber_name.into()));
    }
}

impl<T: IsEvent> Drop for BackgroundEventLoop<T> {
    fn drop(&mut self) {
        self.cancel()
    }
}

#[doc(hidden)]
enum SubscriberOperation<T: IsEvent> {
    Register(Box<dyn Subscriber<T>>),
    Unregister(String),
}

#[doc(hidden)]
struct SharedBackgroundEventLoopState<T: IsEvent> {
    paused: AtomicBool,
    pending_subscribers: proton_async::sync::Mutex<Vec<SubscriberOperation<T>>>,
    token: CancellationToken,
}

#[doc(hidden)]
struct BackgroundLoopState<T: IsEvent> {
    error_handler: Box<dyn EventLoopErrorHandler>,
    shared: Arc<SharedBackgroundEventLoopState<T>>,
    event_loop: EventLoop<T>,
}

#[doc(hidden)]
impl<T: IsEvent> BackgroundLoopState<T> {
    async fn run(&mut self, poll_interval: Duration) {
        debug!("Starting loop");
        loop {
            proton_async::futures::select! {
                _= self.shared.token.cancelled().fuse()=> {
                    debug!("Cancellation requested, exiting");
                    return;
                }

                _= proton_async::time::sleep(poll_interval).fuse() => {
                    self.tick().await
                }
            }
        }
    }

    async fn tick(&mut self) {
        // Process pending subscriber operations
        {
            let mut accessor = self.shared.pending_subscribers.lock().await;
            for operation in accessor.drain(..) {
                match operation {
                    SubscriberOperation::Register(s) => {
                        self.event_loop.add_subscriber(s);
                    }

                    SubscriberOperation::Unregister(s) => {
                        self.event_loop.remove_subscriber(&s);
                    }
                }
            }
        }

        if self.shared.paused.load(Ordering::Acquire) {
            return;
        }

        if let Err(e) = self.event_loop.poll().await {
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
