#![allow(clippy::module_name_repetitions)]

#[cfg(test)]
#[path = "tests/subscriber.rs"]
mod tests;

use async_trait::async_trait;
// avoid namespace conflicts
use crate::{Event, RawEvent};
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
pub trait RawSubscriber: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &'static str;

    /// Handle incoming events.
    async fn on_raw_events(&self, events: &mut [RawEvent]) -> Result<(), SubscriberError>;

    /// Handle refresh event
    async fn on_raw_refresh(&self, event: &RawEvent) -> Result<(), SubscriberError>;
}

/// A collection of subscribers that handle events of a specific type.
pub struct TypedSubscribers<T: Event> {
    name: &'static str,
    subscribers: Vec<Box<dyn Subscriber<T>>>,
}

impl<T: Event> TypedSubscribers<T> {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            subscribers: Vec::new(),
        }
    }

    pub fn add_subscriber(&mut self, subscriber: Box<dyn Subscriber<T>>) {
        self.subscribers.push(subscriber);
    }

    #[must_use]
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    #[must_use]
    pub fn from(subscriber: Box<dyn Subscriber<T>>) -> Box<dyn RawSubscriber>
    where
        T: From<<T as Event>::Response>,
    {
        let mut typed_subscribers = TypedSubscribers::<T>::new(subscriber.name());
        typed_subscribers.add_subscriber(subscriber);

        typed_subscribers.boxed()
    }
}

#[async_trait]
impl<T> RawSubscriber for TypedSubscribers<T>
where
    T: Event + From<<T as Event>::Response>,
{
    fn name(&self) -> &'static str {
        self.name
    }

    async fn on_raw_events(&self, events: &mut [RawEvent]) -> Result<(), SubscriberError> {
        let mut typed_events = events
            .iter()
            .map(RawEvent::deserialize)
            .collect::<Result<Vec<T>, AnyhowError>>()?;

        for subscriber in &self.subscribers {
            subscriber.on_events(&mut typed_events).await?;
        }
        Ok(())
    }

    async fn on_raw_refresh(&self, event: &RawEvent) -> Result<(), SubscriberError> {
        let typed_event = event.deserialize::<T>()?;

        for subscriber in &self.subscribers {
            subscriber.on_refresh(&typed_event).await?;
        }

        Ok(())
    }
}
