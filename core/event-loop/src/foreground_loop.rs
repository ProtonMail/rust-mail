use crate::provider::Provider;
use crate::store::Store;
use crate::subscriber::Subscriber;
use crate::{Event, EventLoopError};
use anyhow::anyhow;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId;
use tracing::{self, debug, error, Level};

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
///
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop. For a continuous loop which operates in the background see
/// [`BackgroundEventLoop`].

#[derive(Debug)]
pub struct EventLoop {}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_EVENTS_PER_POLL: usize = 50;
impl EventLoop {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

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
    #[tracing::instrument(name="event_poll",level=Level::DEBUG, skip(self, store, provider, subscribers))]
    pub async fn poll<T: Event + From<<T as Event>::Response>>(
        &self,
        store: &dyn Store,
        provider: &dyn Provider<T>,
        subscribers: &[Box<dyn Subscriber<T>>],
    ) -> Result<(), EventLoopError> {
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e}");
            return Err(EventLoopError::StoreRead(e));
        };

        debug!("Last Event Id = {last_event_id}");

        let mut events = self
            .collect_events(provider, &last_event_id)
            .await
            .map_err(|e| {
                error!("Failed to collect events: {e}");
                e
            })?;

        {
            let Some(last_event) = events.last() else {
                return Err(EventLoopError::Other("Collected no events".into()));
            };

            if *last_event.event_id() == last_event_id.into() {
                debug!("No new events");
                //no new api events
                return Ok(());
            }
        }

        debug!(
            "Received new events: {:?}",
            events
                .iter()
                .map(|e| e.event_id().clone())
                .collect::<Vec<_>>()
        );

        self.publish_events_to_subscribers(&mut events, subscribers)
            .await?;

        let new_event_id = events
            .last()
            .expect("should be at least one event object present")
            .event_id()
            .clone();

        if let Err(e) = store.store(new_event_id.clone().into()).await {
            error!("Failed to store new event id: {e}");
            return Err(EventLoopError::StoreWrite(e));
        }

        debug!("New Event ID = {}", new_event_id);

        Ok(())
    }

    async fn collect_events<T: Event + From<<T as Event>::Response>>(
        &self,
        provider: &dyn Provider<T>,
        last_event_id: &RemoteId,
    ) -> Result<Vec<T>, ApiServiceError> {
        let mut events = Vec::with_capacity(4);

        let event = provider.get_event(last_event_id).await?;

        let mut has_more = event.has_more();
        let mut next_event_id = event.event_id().clone();
        events.push(event);

        let mut num_collected = 0_usize;

        while has_more {
            num_collected += 1;
            if num_collected >= MAX_EVENTS_PER_POLL {
                return Ok(events);
            }

            let event = provider.get_event(&next_event_id.into()).await?;
            has_more = event.has_more();
            next_event_id = event.event_id().clone();
            events.push(event);
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
                error!("Failed to publish events to '{}': {e}", subscriber.name());
                return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
            }
        }

        Ok(())
    }
}
