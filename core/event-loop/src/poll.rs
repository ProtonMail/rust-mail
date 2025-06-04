use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::RawSubscriber;
use crate::{Event, EventLoopError, RawEvent};
use anyhow::anyhow;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use tokio::sync::Mutex;
use tracing::{self, Level, debug, error};

pub struct EventPoll {
    epoll: EventPollInternal,
    store: Box<dyn Store>,
    provider: Box<dyn Provider>,
    unique_sub: Mutex<HashMap<&'static str, usize>>,
    subscribers: Mutex<Vec<Box<dyn RawSubscriber>>>,
}

impl EventPoll {
    #[must_use]
    pub fn new(store: Box<dyn Store>, provider: Box<dyn Provider>) -> Self {
        let epoll = EventPollInternal::new();

        Self {
            epoll,
            store,
            provider,
            unique_sub: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn initialize(&self) -> Result<&Self, EventLoopError> {
        self.epoll
            .initialize(self.store.as_ref(), self.provider.as_ref())
            .await?;

        Ok(self)
    }

    pub async fn register(
        &self,
        subscriber: Box<dyn RawSubscriber>,
    ) -> Result<&Self, EventLoopError> {
        let mut subscribers = self.subscribers.lock().await;
        match self.unique_sub.lock().await.entry(subscriber.name()) {
            Entry::Occupied(_) => return Err(EventLoopError::Register(subscriber.name())),
            Entry::Vacant(entry) => {
                entry.insert(subscribers.len());
                subscribers.push(subscriber);
            }
        }

        Ok(self)
    }

    pub async fn poll(&self) -> Result<(), EventLoopError> {
        self.epoll
            .poll_raw(
                self.store.as_ref(),
                self.provider.as_ref(),
                self.subscribers.lock().await.as_slice(),
            )
            .await
    }
}

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
///
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop.
#[derive(Debug, Default)]
pub struct EventPollInternal;

const MAX_EVENTS_PER_POLL: usize = 50;
impl EventPollInternal {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Stores one event id if the [`Store`] does not contain an event.
    #[tracing::instrument(name="event_initialize",level=Level::DEBUG, skip(self, store, provider))]
    pub async fn initialize(
        &self,
        store: &dyn Store,
        provider: &dyn Provider,
    ) -> Result<(), EventLoopError> {
        if let Some(e) = store.load().await.map_err(EventLoopError::StoreRead)? {
            debug!("Last event id = {e}");
        } else {
            debug!("No event id in event store, retrieving latest");
            let event_id = provider.get_latest_event_id().await?;
            debug!("Last event id = {event_id}");
            store
                .store(event_id)
                .await
                .map_err(EventLoopError::StoreWrite)?;
        }
        Ok(())
    }

    /// Perform one iteration of the event loop with RawSubscribers, which consists of retrieving the latest events,
    /// publishing raw events to all registered subscribers and storing the event id for the next iteration.
    /// The execution of the loop is aborted on the first error.
    #[tracing::instrument(name="event_poll_raw",level=Level::DEBUG, skip_all)]
    pub async fn poll_raw(
        &self,
        store: &dyn Store,
        provider: &dyn Provider,
        subscribers: &[Box<dyn RawSubscriber>],
    ) -> Result<(), EventLoopError> {
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::StoreRead(e));
        };

        debug!("Last Event Id = {last_event_id}");

        let mut raw_events: Vec<RawEvent> = self
            .collect_raw_events(provider, &last_event_id)
            .await
            .map_err(|e| {
                error!("Failed to collect events: {e:?}");
                e
            })?;

        if raw_events.is_empty() {
            debug!("No new api events");
            return Ok(());
        }

        debug!("Received {} new raw events", raw_events.len());

        // Run 1 tx per event to avoid having long running transactions
        for raw_event in &mut raw_events {
            let new_event_id = raw_event.event_id().clone();
            if raw_event.is_refresh() {
                self.publish_raw_refresh_to_subscribers(raw_event, subscribers)
                    .await?;
            } else {
                self.publish_raw_events_to_subscribers(&mut [raw_event.clone()], subscribers)
                    .await?;
            }

            if let Err(e) = store.store(new_event_id.clone()).await {
                error!("Failed to store new event id: {e:?}");
                return Err(EventLoopError::StoreWrite(e));
            }
            debug!("New Event ID = {}", new_event_id);
        }

        Ok(())
    }

    /// Requests all raw events. The resulting vec may be empty if no new events.
    async fn collect_raw_events(
        &self,
        provider: &dyn Provider,
        last_event_id: &EventId,
    ) -> Result<Vec<RawEvent>, ApiServiceError> {
        let mut events = Vec::with_capacity(4);
        let mut current_event_id = last_event_id.clone();

        for _ in 0..MAX_EVENTS_PER_POLL {
            let event = provider.get_event(&current_event_id).await?;
            let has_more = event.has_more();
            let new_event_id = event.event_id().clone();

            // If this is the same event ID, we don't have new events
            if new_event_id == current_event_id {
                break;
            }

            events.push(event);
            if !has_more {
                break;
            }

            current_event_id = new_event_id;
        }

        Ok(events)
    }

    async fn publish_raw_events_to_subscribers(
        &self,
        events: &mut [RawEvent],
        subscribers: &[Box<dyn RawSubscriber>],
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            if let Err(e) = subscriber.on_raw_events(events).await {
                error!(
                    "Failed to publish raw events to '{}': {e:?}",
                    subscriber.name()
                );
                return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
            }
        }

        Ok(())
    }

    async fn publish_raw_refresh_to_subscribers(
        &self,
        event: &RawEvent,
        subscribers: &[Box<dyn RawSubscriber>],
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            if let Err(e) = subscriber.on_raw_refresh(event).await {
                error!(
                    "Failed to process raw refresh in subscriber '{}': {e:?}",
                    subscriber.name()
                );
                return Err(EventLoopError::Refresh(subscriber.name().to_owned(), e));
            }
        }

        Ok(())
    }
}
