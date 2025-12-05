use crate::{Event, EventLoopError, RawEvent};
use async_trait::async_trait;
use std::any::Any;
use std::error::Error;
use tracing::error;

pub trait SubscriberError: Error + Send + Sync + 'static {
    fn is_network_failure(&self) -> bool;

    fn is_retryable(&self) -> bool {
        self.is_network_failure()
    }
}

pub type SubscriberResult<T> = Result<T, Box<dyn SubscriberError>>;

/// Subscriber traits allow anyone to access the events from the event loop.
#[async_trait]
pub trait Subscriber<T: Event>: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &'static str;

    /// Handle incoming events.
    async fn on_events(&self, event: &mut [T]) -> SubscriberResult<()>;

    /// Handle refresh event
    async fn on_refresh(&self, event: &T) -> SubscriberResult<()>;

    /// Whether or not this should be cleaned up
    fn is_alive(&self) -> bool;
}

/// A trait for subscribers that handle raw events.
///
/// Used for internal events handling before it can be converted to a typed event.
/// This is used to avoid having to convert the events to a concrete type before passing
/// it to the subscribers. For example we have one event provider over multiple subscribers
/// wanting to handle the events in a different way. We keep them as raw events until we have
/// converted them to a concrete type and pass them to the subscribers.
///
///
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub(crate) trait RawSubscriber: Any + Send + Sync {
    /// Handle incoming events.
    async fn on_raw_events(&self, events: &mut [RawEvent]) -> Result<(), EventLoopError>;

    /// Handle refresh event
    async fn on_raw_refresh(&self, event: &RawEvent) -> Result<(), EventLoopError>;

    /// Delete old subscribers
    fn cleanup(&mut self);
}

/// A collection of subscribers that handle events of a specific type.
pub(crate) struct TypedSubscribers<T: Event> {
    subscribers: Vec<Box<dyn Subscriber<T>>>,
}

impl<T: Event + From<<T as Event>::Response>> Default for TypedSubscribers<T> {
    fn default() -> Self {
        Self {
            subscribers: Vec::default(),
        }
    }
}

impl<T: Event> TypedSubscribers<T> {
    #[must_use]
    pub fn new_raw(subscriber: Box<dyn Subscriber<T>>) -> Box<dyn RawSubscriber>
    where
        T: From<<T as Event>::Response>,
    {
        let mut this = TypedSubscribers::<T>::default();

        this.add_subscriber(subscriber);
        this.boxed()
    }

    pub fn add_subscriber(&mut self, subscriber: Box<dyn Subscriber<T>>) {
        self.subscribers.push(subscriber);
    }

    #[must_use]
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

#[async_trait]
impl<T> RawSubscriber for TypedSubscribers<T>
where
    T: Event + From<<T as Event>::Response>,
{
    async fn on_raw_events(&self, events: &mut [RawEvent]) -> Result<(), EventLoopError> {
        let mut typed_events = events
            .iter()
            .map(RawEvent::deserialize)
            .collect::<Result<Vec<T>, anyhow::Error>>()
            .map_err(EventLoopError::Deserialize)?;

        for subscriber in &self.subscribers {
            if let Err(e) = subscriber.on_events(&mut typed_events).await {
                error!("Failed to publish events to '{}': {e:?}", subscriber.name());
                return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
            }
        }
        Ok(())
    }

    async fn on_raw_refresh(&self, event: &RawEvent) -> Result<(), EventLoopError> {
        let typed_event = event
            .deserialize::<T>()
            .map_err(EventLoopError::Deserialize)?;

        for subscriber in &self.subscribers {
            if let Err(e) = subscriber.on_refresh(&typed_event).await {
                error!(
                    "Failed to publish refresh to '{}': {e:?}",
                    subscriber.name()
                );
                return Err(EventLoopError::Refresh(subscriber.name().into(), e));
            }
        }

        Ok(())
    }

    fn cleanup(&mut self) {
        self.subscribers.retain(|s| s.is_alive());
    }
}
