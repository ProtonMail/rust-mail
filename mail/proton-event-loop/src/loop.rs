use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::{Subscriber, SubscriberError};
use proton_api_core::domain::{EventId, IsEvent};
use proton_api_core::exports::anyhow;
use proton_api_core::exports::thiserror;
use proton_api_core::exports::tracing::{debug, error};
use proton_api_core::http;
use proton_api_core::http::HttpRequestError;
use proton_async::tokio;
use proton_async::tokio::time::MissedTickBehavior;
use proton_async::tokio_util::sync::CancellationToken;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] HttpRequestError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
}

const MAX_EVENTS_PER_POLL: usize = 50;

/// Response returned by the `LoopErrorHandler` to control the behavior of the event loop after an error occurs.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LoopErrorHandlerReply {
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
pub trait LoopErrorHandler: Send + Sync {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply;
}

/// This type polls the proton events at a given interval and distributes incoming events among its subscribers.
#[derive(Clone)]
pub struct Loop<T: IsEvent + 'static> {
    inner: Arc<SharedLoopState<T>>,
}
impl<T: IsEvent + 'static> Default for Loop<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: IsEvent + 'static> Loop<T> {
    pub fn new() -> Self {
        let shared = Arc::new(SharedLoopState {
            paused: AtomicBool::new(true),
            pending_subscribers: Default::default(),
            token: CancellationToken::new(),
        });

        Self { inner: shared }
    }

    /// Wait until the event loop has been cancelled. This does not imply the event loop has finished executing, but
    /// a cancel signal was received. The event loop will terminate shortly after that.
    pub async fn wait_on_cancelled(&self) {
        self.inner.token.cancelled().await
    }

    pub async fn start(
        &self,
        interval: Duration,
        store: Box<dyn Store>,
        provider: Box<dyn Provider<T>>,
        error_handler: Box<dyn LoopErrorHandler>,
    ) -> Result<tokio::task::JoinHandle<()>, LoopError> {
        let last_event_id = match store.load().await.map_err(LoopError::StoreRead)? {
            Some(id) => id,
            None => {
                debug!("No event id in event store, retrieving latest");
                let id = provider.get_latest_event_id().await?;
                store.store(&id).await.map_err(LoopError::StoreRead)?;
                id
            }
        };

        let mut loop_state = LoopState {
            store,
            provider,
            shared: self.inner.clone(),
            error_handler,
            subscribers: Vec::new(),
        };

        Ok(tokio::spawn(async move {
            loop_state.run(interval, last_event_id).await
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

impl<T: IsEvent> Drop for Loop<T> {
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
struct SharedLoopState<T: IsEvent> {
    paused: AtomicBool,
    pending_subscribers: tokio::sync::Mutex<Vec<SubscriberOperation<T>>>,
    token: CancellationToken,
}

#[doc(hidden)]
struct LoopState<T: IsEvent> {
    store: Box<dyn Store>,
    provider: Box<dyn Provider<T>>,
    error_handler: Box<dyn LoopErrorHandler>,
    shared: Arc<SharedLoopState<T>>,
    subscribers: Vec<Box<dyn Subscriber<T>>>,
}

#[doc(hidden)]
impl<T: IsEvent> LoopState<T> {
    async fn run(&mut self, poll_interval: Duration, mut last_event_id: EventId) {
        let mut events = Vec::with_capacity(MAX_EVENTS_PER_POLL);

        let interval = tokio::time::interval(poll_interval);
        let mut interval = std::pin::pin!(interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        debug!("Starting loop");
        loop {
            tokio::select! {
                _= self.shared.token.cancelled() => {
                    debug!("Cancellation requested, exiting");
                    return;
                }

                _= interval.tick() => {
                    self.get_and_publish_events(&mut last_event_id, &mut events).await
                }
            }
        }
    }

    async fn collect_events(&self, last_event_id: &EventId, out: &mut Vec<T>) -> http::Result<()> {
        out.clear();

        let event = self.provider.get_event(last_event_id).await?;

        let mut has_more = event.has_more();
        let mut next_event_id = event.event_id().clone();
        out.push(event);

        let mut num_collected = 0_usize;

        while has_more {
            num_collected += 1;
            if num_collected >= MAX_EVENTS_PER_POLL {
                return Ok(());
            }

            let event = self.provider.get_event(&next_event_id).await?;
            has_more = event.has_more();
            next_event_id = event.event_id().clone();
            out.push(event);
        }

        Ok(())
    }

    async fn get_and_publish_events(&mut self, last_event_id: &mut EventId, events: &mut Vec<T>) {
        // Process pending subscriber operations
        {
            let mut accessor = self.shared.pending_subscribers.lock().await;
            for operation in accessor.drain(..) {
                match operation {
                    SubscriberOperation::Register(s) => {
                        let new_subscriber_name = s.name();
                        debug!("Registering subscriber {new_subscriber_name}");
                        if !self
                            .subscribers
                            .iter()
                            .any(move |v| v.name() == new_subscriber_name)
                        {
                            self.subscribers.push(s);
                        }
                    }

                    SubscriberOperation::Unregister(s) => {
                        debug!("Unregistering subscriber {s}");
                        self.subscribers.retain(|v| v.name() != s);
                    }
                }
            }
        }

        if self.shared.paused.load(Ordering::Acquire) {
            return;
        }

        if let Err(e) = self.collect_events(last_event_id, events).await {
            self.on_error(LoopError::Provider(e));
            return;
        }

        if *events
            .last()
            .expect("should be at least one event object present")
            .event_id()
            == *last_event_id
        {
            debug!("No new events");
            //no new api events
            return;
        }

        debug!(
            "Received new events: {:?}",
            events
                .iter()
                .map(|e| e.event_id().clone())
                .collect::<Vec<_>>()
        );

        if let Err(e) = self.publish_events_to_subscribers(events).await {
            self.on_error(e);
            return;
        }

        let new_event_id = events
            .last()
            .expect("should be at least one event object present")
            .event_id()
            .clone();
        if let Err(e) = self.store.store(&new_event_id).await {
            self.on_error(LoopError::StoreWrite(e));
            return;
        }

        *last_event_id = new_event_id;
    }

    fn on_error(&mut self, error: LoopError) {
        match self.error_handler.on_error(error) {
            LoopErrorHandlerReply::Pause => {
                self.shared.paused.store(true, Ordering::Release);
            }
            LoopErrorHandlerReply::Retry => {
                // Nothing to do
            }
            LoopErrorHandlerReply::Abort => {
                self.shared.token.cancel();
            }
        }
    }

    async fn publish_events_to_subscribers(&mut self, events: &[T]) -> Result<(), LoopError> {
        for subscriber in &mut self.subscribers {
            if let Err(e) = subscriber.on_events(events).await {
                error!("Failed to publish events to '{}': {e}", subscriber.name());
                return Err(LoopError::Subscriber(subscriber.name().into(), e));
            }
        }

        Ok(())
    }
}
