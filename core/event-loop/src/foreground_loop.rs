use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::Subscriber;
use crate::{Event, EventLoopError};
use anyhow::anyhow;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use tokio::sync::Mutex;
use tracing::{self, Level, debug, error};

pub struct EventLoop<T: Send + Sync> {
    eloop: EventLoopInternal,
    store: Box<dyn Store>,
    provider: Box<dyn Provider>,
    uniqe_sub: Mutex<HashMap<&'static str, usize>>,
    subscribers: Mutex<Vec<Box<dyn Subscriber<T>>>>,
}

impl<T: Event + From<<T as Event>::Response>> EventLoop<T> {
    #[must_use]
    pub fn new(store: Box<dyn Store>, provider: Box<dyn Provider>) -> Self {
        let eloop = EventLoopInternal::new();

        Self {
            eloop,
            store,
            provider,
            uniqe_sub: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn initialize(&self) -> Result<&Self, EventLoopError> {
        self.eloop
            .initialize(self.store.as_ref(), self.provider.as_ref())
            .await?;

        Ok(self)
    }

    pub async fn register(
        &self,
        subscriber: Box<dyn Subscriber<T>>,
    ) -> Result<&Self, EventLoopError> {
        let mut subscribers = self.subscribers.lock().await;
        match self.uniqe_sub.lock().await.entry(subscriber.name()) {
            Entry::Occupied(_) => return Err(EventLoopError::Register(subscriber.name())),
            Entry::Vacant(entry) => {
                entry.insert(subscribers.len());
                subscribers.push(subscriber);
            }
        }

        Ok(self)
    }

    pub async fn poll(&self) -> Result<(), EventLoopError> {
        self.eloop
            .poll::<T>(
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
pub struct EventLoopInternal;

const MAX_EVENTS_PER_POLL: usize = 50;
impl EventLoopInternal {
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

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it all the registered subscribers and storing the event id for the next
    /// iteration.
    /// The execution of the loop is aborted on the first error.
    #[tracing::instrument(name="event_poll",level=Level::DEBUG, skip_all)]
    pub async fn poll<T: Event + From<<T as Event>::Response>>(
        &self,
        store: &dyn Store,
        provider: &dyn Provider,
        subscribers: &[Box<dyn Subscriber<T>>],
    ) -> Result<(), EventLoopError> {
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::StoreRead(e));
        };

        debug!("Last Event Id = {last_event_id}");

        let events: Vec<T> = self
            .collect_events(provider, &last_event_id)
            .await
            .map_err(|e| {
                error!("Failed to collect events: {e:?}");
                e
            })?;

        if *events
            .last()
            .expect("collect_events must collect at least one event")
            .event_id()
            == last_event_id
        {
            debug!("No new api events");
            return Ok(());
        }

        debug!(
            "Received new events: {:?}",
            events.iter().map(Event::event_id).collect::<Vec<_>>()
        );

        // Run 1 tx per event to avoid having long running transactions. Under normal circumstances
        // this is not really an issue, but with the current iOS setup, if we enter a background
        // state and we allow transactions to finish we can get killed by the OS. On Average
        // the grace period seems to be around 200ms. It has been observed that on large events,
        // the whole process can take > 200ms together.
        for event in events {
            let new_event_id = event.event_id().clone();
            if event.is_refresh() {
                self.publish_refresh_to_subscribers(&event, subscribers)
                    .await?;
            } else {
                self.publish_events_to_subscribers(&mut [event], subscribers)
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

    /// Requests all events. The resulting vec is non empty.
    async fn collect_events<T: Event + From<<T as Event>::Response>>(
        &self,
        provider: &dyn Provider,
        mut last_event_id: &EventId,
    ) -> Result<Vec<T>, ApiServiceError> {
        let mut events = Vec::with_capacity(4);

        for _ in 0..MAX_EVENTS_PER_POLL {
            let event = provider
                .get_event(last_event_id)
                .await?
                .deserialize::<T>()?;
            let has_more = event.has_more();
            events.push(event);
            if !has_more {
                break;
            }

            last_event_id = events.last().unwrap().event_id();
        }

        Ok(events)
    }

    async fn publish_events_to_subscribers<T: Event>(
        &self,
        events: &mut [T],
        subscribers: &[Box<dyn Subscriber<T>>],
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            if let Err(e) = subscriber.on_events(events).await {
                error!("Failed to publish events to '{}': {e:?}", subscriber.name());
                return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
            }
        }

        Ok(())
    }

    async fn publish_refresh_to_subscribers<T: Event>(
        &self,
        event: &T,
        subscribers: &[Box<dyn Subscriber<T>>],
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            if let Err(e) = subscriber.on_refresh(event).await {
                error!(
                    "Failed to publish refresh to '{}': {e:?}",
                    subscriber.name()
                );
                return Err(EventLoopError::Refresh(subscriber.name().into(), e));
            }
        }

        Ok(())
    }
}
