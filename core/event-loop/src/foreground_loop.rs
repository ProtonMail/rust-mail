use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::Subscriber;
use crate::{Event, EventLoopError};
use anyhow::anyhow;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::EventId;
use tracing::{self, Level, debug, error};

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
///
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop. For a continuous loop which operates in the background see
/// [`BackgroundEventLoop`].
#[derive(Debug, Default)]
pub struct EventLoop;

const MAX_EVENTS_PER_POLL: usize = 50;
impl EventLoop {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Stores one event id if the [`Store`] does not contain an event.
    #[tracing::instrument(name="event_initialize",level=Level::DEBUG, skip(self, store, provider))]
    pub async fn initialize<T: Event + From<<T as Event>::Response>>(
        &self,
        store: &dyn Store,
        provider: &dyn Provider<T>,
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
        provider: &dyn Provider<T>,
        subscribers: &[Box<dyn Subscriber<T>>],
    ) -> Result<(), EventLoopError> {
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::StoreRead(e));
        };

        debug!("Last Event Id = {last_event_id}");

        let events = self
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

        if events.iter().any(Event::is_refresh) {
            error!("Received refresh event, but this is not yet implemented");
            return Err(EventLoopError::Refresh);
        }

        // Run 1 tx per event to avoid having long running transactions. Under normal circumstances
        // this is not really an issue, but with the current iOS setup, if we enter a background
        // state and we allow transactions to finish we can get killed by the OS. On Average
        // the grace period seems to be around 200ms. It has been observed that on large events,
        // the whole process can take > 200ms together.
        for event in events {
            let new_event_id = event.event_id().clone();
            self.publish_events_to_subscribers(&mut [event], subscribers)
                .await?;

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
        provider: &dyn Provider<T>,
        mut last_event_id: &EventId,
    ) -> Result<Vec<T>, ApiServiceError> {
        let mut events = Vec::with_capacity(4);

        for _ in 0..MAX_EVENTS_PER_POLL {
            let event = provider.get_event(last_event_id).await?;
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
}
