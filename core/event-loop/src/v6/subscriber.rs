use crate::v6::source::EventSource;
use crate::{EventLoopError, MAX_ERROR_RETRIES, RawEvent, RefreshFlag};
use async_trait::async_trait;
use slotmap::{DefaultKey, SlotMap};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use tracing::error;

#[derive(Debug)]
#[repr(transparent)]
pub struct EventSubscriberId<E: EventSource> {
    handle: DefaultKey,
    _p: PhantomData<E>,
}

impl<E: EventSource> Clone for EventSubscriberId<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: EventSource> Copy for EventSubscriberId<E> {}

impl<E: EventSource> PartialEq for EventSubscriberId<E> {
    fn eq(&self, other: &Self) -> bool {
        self.handle.eq(&other.handle)
    }
}

impl<E: EventSource> Eq for EventSubscriberId<E> {}

impl<E: EventSource> PartialOrd for EventSubscriberId<E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<E: EventSource> Ord for EventSubscriberId<E> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.handle.cmp(&other.handle)
    }
}

impl<E: EventSource> Hash for EventSubscriberId<E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.handle.hash(state);
    }
}

#[cfg_attr(test, allow(clippy::ref_option_ref))]
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EventSubscriber<E: EventSource>: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    /// Invoked when an event has been fetched from the server.
    async fn on_event(&self, event: &E::Event, cache: &mut E::Cache) -> EventSubscriberResult<()>;

    /// Invoked either when a refresh event was retrieved from the server or if manually requested.
    /// In the latter case, there is no `event` object.
    async fn on_refresh(
        &self,
        event: RefreshFlag,
        cache: &mut E::Cache,
    ) -> EventSubscriberResult<()>;
}

#[cfg_attr(test, allow(clippy::ref_option_ref))]
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub(crate) trait SubscriberList: Any + Send + Sync {
    async fn on_event(&self, events: &RawEvent) -> Result<(), EventLoopError>;
    async fn on_refresh(&self, refresh: RefreshFlag) -> Result<(), EventLoopError>;
}

/// A collection of subscribers that handle events of a specific type.
pub(crate) struct TypedSubscriberList<E: EventSource> {
    subscribers: SlotMap<DefaultKey, Box<dyn EventSubscriber<E>>>,
    subscriber_names: HashSet<String>,
}

impl<E: EventSource> Default for TypedSubscriberList<E> {
    fn default() -> Self {
        Self {
            subscribers: SlotMap::new(),
            subscriber_names: HashSet::new(),
        }
    }
}

impl<E: EventSource> TypedSubscriberList<E> {
    pub fn add(&mut self, subscriber: Box<dyn EventSubscriber<E>>) -> Option<EventSubscriberId<E>> {
        if !self.subscriber_names.insert(subscriber.name().to_owned()) {
            return None;
        }
        tracing::info!("Adding event subscriber {}", subscriber.name());
        let handle = self.subscribers.insert(subscriber);
        Some(EventSubscriberId {
            handle,
            _p: PhantomData,
        })
    }

    pub fn remove(&mut self, key: EventSubscriberId<E>) -> Option<Box<dyn EventSubscriber<E>>> {
        self.subscribers.remove(key.handle).inspect(|previous| {
            tracing::info!("Removing event subscriber {}", previous.name());
            self.subscriber_names.remove(previous.name());
        })
    }

    #[must_use]
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

#[async_trait]
impl<E> SubscriberList for TypedSubscriberList<E>
where
    E: EventSource,
{
    async fn on_event(&self, event: &RawEvent) -> Result<(), EventLoopError> {
        let event = RawEvent::deserialize::<E::Event>(event)
            .map_err(|e| EventLoopError::Deserialize(anyhow::Error::new(e)))?;
        let mut cache = E::Cache::default();

        for subscriber in self.subscribers.values() {
            let mut num_attempts = 0;
            while let Err(e) = subscriber.on_event(&event, &mut cache).await {
                error!(
                    "Failed to apply events to '{}' (attempt:{num_attempts}): {e:?}",
                    subscriber.name()
                );
                num_attempts += 1;
                if num_attempts == MAX_ERROR_RETRIES || !e.is_retryable() {
                    return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
                }
            }
        }
        Ok(())
    }

    async fn on_refresh(&self, refresh: RefreshFlag) -> Result<(), EventLoopError> {
        let mut cache = E::Cache::default();
        for subscriber in self.subscribers.values() {
            let mut num_attempts = 0;
            while let Err(e) = subscriber.on_refresh(refresh, &mut cache).await {
                error!(
                    "Failed to apply refresh to '{}' (attempt:{num_attempts}): {e:?}",
                    subscriber.name()
                );
                num_attempts += 1;
                if num_attempts == MAX_ERROR_RETRIES || !e.is_retryable() {
                    return Err(EventLoopError::Refresh(subscriber.name().into(), e));
                }
            }
        }

        Ok(())
    }
}

pub trait EventSubscriberError: Error + Send + Sync + 'static {
    fn is_network_failure(&self) -> bool;

    fn is_retryable(&self) -> bool {
        self.is_network_failure()
    }
}

pub type EventSubscriberResult<T> = Result<T, Box<dyn EventSubscriberError>>;
