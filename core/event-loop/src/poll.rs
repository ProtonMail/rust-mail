use crate::store::Store;
use crate::subscriber::{RawSubscriber, TypedSubscribers};
use crate::{Event, EventLoopError, Provider, RawEvent, Subscriber};
use anyhow::anyhow;
use indexmap::{IndexMap, map::Entry};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use std::any::{Any, TypeId, type_name};
use tokio::sync::Mutex;
use tracing::{self, Level, debug, error, info};

pub struct EventPoll {
    epoll: EventPollInternal,
    store: Box<dyn Store>,
    provider: Box<dyn Provider>,
    /// The subscribers are stored in a indexmap of boxed raw subscribers.
    /// The indexmap was chosen to preserve the order of the subscribers to run - FIFO.
    /// The indexmap stores the type id of the subscriber to allow for multiple subscribers
    /// of the same type to prevent double deserialization of the same event.
    subscribers: Mutex<IndexMap<TypeId, Box<dyn RawSubscriber>>>,
}

impl EventPoll {
    #[must_use]
    pub fn new(store: Box<dyn Store>, provider: Box<dyn Provider>) -> Self {
        let epoll = EventPollInternal::new();

        Self {
            epoll,
            store,
            provider,
            subscribers: Mutex::new(IndexMap::new()),
        }
    }

    pub async fn initialize(&self) -> Result<&Self, EventLoopError> {
        self.epoll
            .initialize(self.store.as_ref(), self.provider.as_ref())
            .await?;

        Ok(self)
    }

    /// Register a typed subscriber by wrapping it in `TypedSubscribers`.
    ///
    /// This is used to register a typed subscriber to the event loop.
    /// The subscriber is wrapped in a `TypedSubscribers` to allow for multiple subscribers
    /// of the same type.
    ///
    pub async fn register<T: Event + From<<T as Event>::Response>>(
        &self,
        subscriber: Box<dyn Subscriber<T>>,
    ) -> Result<&Self, EventLoopError> {
        let mut subscribers = self.subscribers.lock().await;
        match subscribers.entry(TypeId::of::<T>()) {
            Entry::Occupied(mut entry) => {
                if let Some(typed_subscribers) =
                    <dyn Any>::downcast_mut::<TypedSubscribers<T>>(&mut *entry.get_mut())
                {
                    typed_subscribers.add_subscriber(subscriber);
                } else {
                    // This should never happen, as the raw subscriber is already registered
                    error!(
                        "Subscriber could not be downcast to TypedSubscribers<{}>",
                        type_name::<T>()
                    );
                    return Err(EventLoopError::Register(subscriber.name()));
                }
            }
            Entry::Vacant(_) => {
                let raw_subscriber = TypedSubscribers::<T>::new_raw(subscriber);
                subscribers.insert(TypeId::of::<T>(), raw_subscriber);
            }
        }

        Ok(self)
    }

    pub async fn poll(&self) -> Result<(), EventLoopError> {
        {
            let mut l = self.subscribers.lock().await;
            for s in l.values_mut() {
                s.cleanup();
            }
        }

        self.epoll
            .poll_raw(
                self.store.as_ref(),
                self.provider.as_ref(),
                &*self.subscribers.lock().await,
                MAX_EVENTS_PER_POLL,
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
            info!("Last event id = {e}");
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
    #[tracing::instrument(name="event_poll_raw", level=Level::DEBUG, skip_all)]
    pub(crate) async fn poll_raw(
        &self,
        store: &dyn Store,
        provider: &dyn Provider,
        subscribers: &IndexMap<TypeId, Box<dyn RawSubscriber>>,
        max_events: usize,
    ) -> Result<(), EventLoopError> {
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::StoreRead(e));
        };

        info!("Last Event Id = {last_event_id}");
        let mut previous_event_id = last_event_id.clone();

        let mut processed_event_count = 0_usize;

        while processed_event_count < max_events {
            let raw_events: Vec<RawEvent> = self
                .collect_raw_events(provider, &previous_event_id)
                .await
                .map_err(|e| {
                    error!("Failed to collect events: {e}");
                    e
                })?;

            if raw_events.is_empty() {
                info!("No new api events");
                return Ok(());
            }

            let raw_event_count = raw_events.len();

            info!("Received {} new events", raw_event_count);
            // Run events 1 at a time to make sure dependencies are processed correctly.
            for raw_event in raw_events {
                let new_event_id = raw_event.event_id().clone();
                info!("Applying {:?}", previous_event_id);
                if raw_event.is_refresh() {
                    self.publish_raw_refresh_to_subscribers(&raw_event, subscribers.values())
                        .await?;
                } else {
                    self.publish_raw_events_to_subscribers(&mut [raw_event], subscribers.values())
                        .await?;
                }

                if let Err(e) = store.store(new_event_id.clone()).await {
                    error!("Failed to store new event id: {e}");
                    return Err(EventLoopError::StoreWrite(e));
                }
                info!("New Event ID = {}", new_event_id);
                previous_event_id = new_event_id;
            }

            processed_event_count += raw_event_count;
        }

        Ok(())
    }

    /// Collect events that are part of the same "group"/update.
    ///
    /// If the events are related to a state update the `has_more` field will be set to true.
    ///
    /// E.g.:
    /// ```skip
    /// Event1 {id:2, has_more=true}  \
    ///                                +- related
    /// Event2 {id:3, has_more=false} /
    /// Event3 {id:4, has_more=false} - new event unrelated
    /// Event4 {id:4, has_more=false} - no more events
    /// ```
    async fn collect_raw_events(
        &self,
        provider: &dyn Provider,
        last_event_id: &EventId,
    ) -> Result<Vec<RawEvent>, ApiServiceError> {
        let mut events = Vec::with_capacity(4);
        let mut current_event_id = last_event_id.clone();

        loop {
            let event = provider.get_event(&current_event_id).await?;
            let has_more = event.has_more();
            let new_event_id = event.event_id().clone();

            // If this is the same event ID, we don't have new events
            if new_event_id == current_event_id {
                break;
            }

            events.push(event);

            // This event "group" is now complete, lets process it before
            // collecting more
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
        subscribers: impl Iterator<Item = &Box<dyn RawSubscriber>>,
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            subscriber.on_raw_events(events).await?;
        }

        Ok(())
    }

    async fn publish_raw_refresh_to_subscribers(
        &self,
        event: &RawEvent,
        subscribers: impl Iterator<Item = &Box<dyn RawSubscriber>>,
    ) -> Result<(), EventLoopError> {
        for subscriber in subscribers {
            subscriber.on_raw_refresh(event).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventMetadata;
    use crate::provider::MockProvider;
    use crate::store::MockStore;
    use crate::subscriber::MockRawSubscriber;
    use mockall::predicate;

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn event_collection() {
        // Events 1 & 2 should be processed together.
        // Event 3 is processed in the next loop.
        // Event 4 terminates the loop.
        let event_id_1 = EventId::from("1");
        let event_id_2 = EventId::from("2");
        let event_id_3 = EventId::from("3");
        let event_id_4 = EventId::from("4");

        let raw_event_1 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_2.clone(),
                has_more: true,
                refresh: 0,
            },
            raw: String::new(),
        };
        let raw_event_2 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_3.clone(),
                has_more: false,
                refresh: 0,
            },
            raw: String::new(),
        };

        let raw_event_3 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_4.clone(),
                has_more: false,
                refresh: 0,
            },
            raw: String::new(),
        };

        let raw_event_4 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_4.clone(),
                has_more: false,
                refresh: 0,
            },
            raw: String::new(),
        };

        let mut sequence = mockall::Sequence::new();
        let mut provider = MockProvider::new();
        let mut subscriber = MockRawSubscriber::new();
        let mut store = MockStore::new();

        let event_id = event_id_1.clone();
        store
            .expect_load()
            .once()
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(event_id.clone())));

        // First loop
        let event = raw_event_1.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_1.clone()))
            .returning(move |_| Ok(event.clone()));
        let event = raw_event_2.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2.clone()))
            .returning(move |_| Ok(event.clone()));
        let id = event_id_2.clone();
        subscriber
            .expect_on_raw_events()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::function(move |v: &[RawEvent]| {
                v[0].meta.event_id == id
            }))
            .returning(|_| Ok(()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2))
            .returning(|_| Ok(()));
        let id = event_id_3.clone();
        subscriber
            .expect_on_raw_events()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::function(move |v: &[RawEvent]| {
                v[0].meta.event_id == id
            }))
            .returning(|_| Ok(()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_3.clone()))
            .returning(|_| Ok(()));

        // Second loop
        let event = raw_event_3.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_3.clone()))
            .returning(move |_| Ok(event.clone()));
        let id = event_id_4.clone();
        subscriber
            .expect_on_raw_events()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::function(move |v: &[RawEvent]| {
                v[0].meta.event_id == id
            }))
            .returning(|_| Ok(()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_4.clone()))
            .returning(|_| Ok(()));

        // Exit
        let event = raw_event_4.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_4))
            .returning(move |_| Ok(event.clone()));

        let evt_poll = EventPollInternal::new();

        let mut subscribers: IndexMap<TypeId, Box<dyn RawSubscriber>> = IndexMap::new();
        subscribers.insert(TypeId::of::<i32>(), Box::new(subscriber));
        evt_poll
            .poll_raw(&store, &provider, &subscribers, 10)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn events_that_exceed_limit_with_has_more_are_still_collected() {
        // Event 1 & 2 should be processed together
        // Event 3 is not processed since we exceed the limit.
        let event_id_1 = EventId::from("1");
        let event_id_2 = EventId::from("2");
        let event_id_3 = EventId::from("3");

        let raw_event_1 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_2.clone(),
                has_more: true,
                refresh: 0,
            },
            raw: String::new(),
        };
        let raw_event_2 = RawEvent {
            meta: EventMetadata {
                event_id: event_id_3.clone(),
                has_more: false,
                refresh: 0,
            },
            raw: String::new(),
        };

        let mut sequence = mockall::Sequence::new();
        let mut provider = MockProvider::new();
        let mut subscriber = MockRawSubscriber::new();
        let mut store = MockStore::new();

        let event_id = event_id_1.clone();
        store
            .expect_load()
            .once()
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(event_id.clone())));

        // First loop
        let event = raw_event_1.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_1.clone()))
            .returning(move |_| Ok(event.clone()));
        let event = raw_event_2.clone();
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2.clone()))
            .returning(move |_| Ok(event.clone()));
        let id = event_id_2.clone();
        subscriber
            .expect_on_raw_events()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::function(move |v: &[RawEvent]| {
                v[0].meta.event_id == id
            }))
            .returning(|_| Ok(()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2))
            .returning(|_| Ok(()));
        let id = event_id_3.clone();
        subscriber
            .expect_on_raw_events()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::function(move |v: &[RawEvent]| {
                v[0].meta.event_id == id
            }))
            .returning(|_| Ok(()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_3.clone()))
            .returning(|_| Ok(()));

        let evt_poll = EventPollInternal::new();

        let mut subscribers: IndexMap<TypeId, Box<dyn RawSubscriber>> = IndexMap::new();
        subscribers.insert(TypeId::of::<i32>(), Box::new(subscriber));
        evt_poll
            .poll_raw(&store, &provider, &subscribers, 1)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn init_fetches_event_id_if_it_does_not_exist() {
        let event_id_1 = EventId::from("1");

        let mut sequence = mockall::Sequence::new();
        let mut provider = MockProvider::new();
        let mut store = MockStore::new();

        let event_id = event_id_1.clone();
        store
            .expect_load()
            .once()
            .in_sequence(&mut sequence)
            .returning(move || Ok(None));

        // First time fetch and store,
        let id = event_id.clone();
        provider
            .expect_get_latest_event_id()
            .once()
            .in_sequence(&mut sequence)
            .returning(move || Ok(id.clone()));
        store
            .expect_store()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_1.clone()))
            .returning(|_| Ok(()));

        // 2nd time there is no fetch
        let event_id = event_id_1.clone();
        store
            .expect_load()
            .once()
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(event_id.clone())));

        let evt_poll = EventPollInternal::new();

        evt_poll.initialize(&store, &provider).await.unwrap();
        evt_poll.initialize(&store, &provider).await.unwrap();
    }
}
