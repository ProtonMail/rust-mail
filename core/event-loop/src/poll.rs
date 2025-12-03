use crate::provider::ProviderResult;
use crate::store::Store;
use crate::subscriber::RawSubscriber;
use crate::{Event, EventId, EventLoopError, Provider, RawEvent};
use anyhow::{Context, anyhow};
use indexmap::IndexMap;
use std::any::TypeId;
use tracing::{self, Level, debug, error, info};

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
///
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop.
#[derive(Debug, Default)]
pub(crate) struct EventPollInternal;

pub(crate) const MAX_EVENTS_PER_POLL: usize = 50;
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
        if let Some(e) = store
            .load()
            .await
            .context("Failed to load event id (init)")
            .map_err(EventLoopError::Store)?
        {
            info!("Last event id = {e}");
        } else {
            debug!("No event id in event store, retrieving latest");
            let event_id = provider.get_latest_event_id().await?;
            debug!("Last event id = {event_id}");
            store
                .store(event_id)
                .await
                .context("Failed to store event id (init)")
                .map_err(EventLoopError::Store)?;
        }
        Ok(())
    }

    /// Perform one iteration of the event loop with `RawSubscribers`, which consists of retrieving the latest events,
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
        let Some(last_event_id) = store
            .load()
            .await
            .context("Failed to load event id (poll)")
            .map_err(EventLoopError::Store)?
        else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::Store(e));
        };

        info!("Last Event Id = {last_event_id}");
        let mut previous_event_id = last_event_id.clone();

        let mut processed_event_count = 0_usize;
        let mut has_more = true;

        // Keep collecting events from the same group, even if we exceed the upper limit
        //
        // If the events are related to a state update the `has_more` field will be set to true.
        //
        // E.g.:
        // ```skip
        // Event1 {id:2, has_more=true}  \
        //                                +- related
        // Event2 {id:3, has_more=false} /
        // Event3 {id:4, has_more=false} - new event unrelated
        // Event4 {id:4, has_more=false} - no more events
        // ```
        while processed_event_count < max_events || has_more {
            let Some(raw_event) = self
                .fetch_event(provider, &previous_event_id)
                .await
                .inspect_err(|e| {
                    error!("Failed to collect events: {e}");
                })?
            else {
                info!("No new api events");
                return Ok(());
            };

            info!("Received new event");
            let new_event_id = raw_event.event_id();
            has_more = raw_event.has_more();
            info!("Applying {:?}", previous_event_id);
            if raw_event.is_refresh() {
                self.publish_raw_refresh_to_subscribers(&raw_event, subscribers.values())
                    .await?;
            } else {
                self.publish_raw_events_to_subscribers(&mut [raw_event], subscribers.values())
                    .await?;
            }

            if let Err(e) = store
                .store(new_event_id.clone())
                .await
                .context("Failed to store event id (poll)")
            {
                error!("Failed to store new event id: {e}");
                return Err(EventLoopError::Store(e));
            }
            info!("New Event ID = {}", new_event_id);
            previous_event_id = new_event_id;
            processed_event_count += 1;
        }

        Ok(())
    }

    async fn fetch_event(
        &self,
        provider: &dyn Provider,
        last_event_id: &EventId,
    ) -> ProviderResult<Option<RawEvent>> {
        let event = provider.get_event(last_event_id).await?;
        let new_event_id = event.event_id();

        // If this is the same event ID, we don't have new events
        if new_event_id == *last_event_id {
            Ok(None)
        } else {
            Ok(Some(event))
        }
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
            .with(predicate::eq(event_id_2.clone()))
            .returning(|_| Ok(()));
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2.clone()))
            .returning(move |_| Ok(event.clone()));
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
            .with(predicate::eq(event_id_2.clone()))
            .returning(|_| Ok(()));
        provider
            .expect_get_event()
            .once()
            .in_sequence(&mut sequence)
            .with(predicate::eq(event_id_2.clone()))
            .returning(move |_| Ok(event.clone()));
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
