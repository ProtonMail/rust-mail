#![allow(clippy::module_name_repetitions)]

#[cfg(test)]
#[path = "tests/subscriber.rs"]
mod tests;

use std::any::Any;

use async_trait::async_trait;
use tracing::error;
// avoid namespace conflicts
use crate::{Event, EventLoopError, RawEvent};
use anyhow::Error as AnyhowError;
use proton_core_api::service::ApiServiceError;
use stash::stash::StashError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubscriberError {
    /// API error should be returned when the error resulted due to an API or Network error.
    #[error("{0}")]
    Api(#[from] ApiServiceError),
    /// Subscriber specific errors should be returned here.
    #[error("{0}")]
    Other(AnyhowError),
    /// Failed to send to the subscriber.
    #[error("Failed to send data to subscriber")]
    Send,
    /// Failed to receive data from subscriber.
    #[error("Failed to receive data from subscriber")]
    Receive,
    /// Stash error, i.e. database error.
    #[error("{0}")]
    StashError(#[from] StashError),
}

impl From<AnyhowError> for SubscriberError {
    fn from(value: AnyhowError) -> Self {
        Self::Other(value)
    }
}

/// Subscriber traits allow anyone to access the events from the event loop.
#[async_trait]
pub trait Subscriber<T: Event>: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &'static str;

    /// Handle incoming events.
    async fn on_events(&self, event: &mut [T]) -> Result<(), SubscriberError>;

    /// Handle refresh event
    async fn on_refresh(&self, event: &T) -> Result<(), SubscriberError>;
}

/// A trait for subscribers that handle raw events.
///
/// Used for internal events handling before it can be converted to a typed event.
/// This is used to avoid having to convert the events to a concrete type before passing
/// it to the subscribers. For example we have one event provider over multiple subscribers
/// wanting to handle the events in a different way. We keep them as raw events until we have
/// converted them to a concrete type and pass them to the subscribers.
///
#[async_trait]
pub trait RawSubscriber: Any + Send + Sync {
    /// Handle incoming events.
    async fn on_raw_events(&self, events: &mut [RawEvent]) -> Result<(), EventLoopError>;

    /// Handle refresh event
    async fn on_raw_refresh(&self, event: &RawEvent) -> Result<(), EventLoopError>;

    fn as_any(&self) -> &dyn Any;

    /// Get mutable reference to self as Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A collection of subscribers that handle events of a specific type.
pub struct TypedSubscribers<T: Event> {
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
        let mut typed_subscribers = TypedSubscribers::<T>::default();
        typed_subscribers.add_subscriber(subscriber);

        typed_subscribers.boxed()
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
            .collect::<Result<Vec<T>, AnyhowError>>()
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
