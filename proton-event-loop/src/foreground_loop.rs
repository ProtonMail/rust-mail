use crate::{EventLoopError, Provider, Store, Subscriber};
use proton_api_core::domain::{EventId, IsEvent};
use proton_api_core::exports::tracing::{self, debug, error};
use proton_api_core::http;

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop. For a continuous loop which operates in the background see
/// [`BackgroundEventLoop`].
pub struct EventLoop<T: IsEvent> {
    last_event_id: EventId,
    store: Box<dyn Store>,
    provider: Box<dyn Provider<T>>,
    subscribers: Vec<Box<dyn Subscriber<T>>>,
    events: Vec<T>,
}

const MAX_EVENTS_PER_POLL: usize = 50;
impl<T: IsEvent> EventLoop<T> {
    pub async fn new(
        store: Box<dyn Store>,
        provider: Box<dyn Provider<T>>,
    ) -> Result<Self, EventLoopError> {
        let last_event_id = if let Some(id) = store.load().map_err(EventLoopError::StoreRead)? {
            id
        } else {
            debug!("No event id in event store, retrieving latest");
            let event_id = provider.get_latest_event_id().await?;
            store.store(&event_id).map_err(EventLoopError::StoreWrite)?;
            event_id
        };

        Ok(Self {
            last_event_id,
            store,
            provider,
            subscribers: Vec::new(),
            events: Vec::with_capacity(MAX_EVENTS_PER_POLL),
        })
    }

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it all the registered subscribers and storing the event id for the next
    /// iteration.
    /// The execution of the loop is aborted on the first error.
    #[tracing::instrument(skip(self), fields(last_event_id = ?self.last_event_id))]
    pub async fn poll(&mut self) -> Result<(), EventLoopError> {
        if let Err(e) = self.collect_events().await {
            error!("Failed to collect events: {e}");
            return Err(e.into());
        }

        {
            let Some(last_event) = self.events.last() else {
                return Err(EventLoopError::Other("Collected no events".into()));
            };

            if *last_event.event_id() == self.last_event_id {
                debug!("No new events");
                //no new api events
                return Ok(());
            }
        }

        debug!(
            "Received new events: {:?}",
            self.events
                .iter()
                .map(|e| e.event_id().clone())
                .collect::<Vec<_>>()
        );

        self.publish_events_to_subscribers().await?;

        let new_event_id = self
            .events
            .last()
            .expect("should be at least one event object present")
            .event_id()
            .clone();

        if let Err(e) = self.store.store(&new_event_id) {
            error!("Failed to store new event id: {e}");
            return Err(EventLoopError::StoreWrite(e));
        }

        self.last_event_id = new_event_id;
        debug!("New Event ID = {}", self.last_event_id);

        Ok(())
    }

    /// Add a new subscriber.
    pub fn add_subscriber(&mut self, subscriber: Box<dyn Subscriber<T>>) {
        let new_subscriber_name = subscriber.name();
        if !self
            .subscribers
            .iter()
            .any(move |v| v.name() == new_subscriber_name)
        {
            debug!("Registering subscriber {new_subscriber_name}");
            self.subscribers.push(subscriber);
        }
    }

    /// Remove a subscriber.
    pub fn remove_subscriber(&mut self, s: &str) {
        debug!("Unregistering subscriber {s}");
        self.subscribers.retain(|v| v.name() != s);
    }

    async fn collect_events(&mut self) -> http::Result<()> {
        self.events.clear();

        let event = self.provider.get_event(&self.last_event_id).await?;

        let mut has_more = event.has_more();
        let mut next_event_id = event.event_id().clone();
        self.events.push(event);

        let mut num_collected = 0_usize;

        while has_more {
            num_collected += 1;
            if num_collected >= MAX_EVENTS_PER_POLL {
                return Ok(());
            }

            let event = self.provider.get_event(&next_event_id).await?;
            has_more = event.has_more();
            next_event_id = event.event_id().clone();
            self.events.push(event);
        }

        Ok(())
    }

    async fn publish_events_to_subscribers(&mut self) -> Result<(), EventLoopError> {
        for subscriber in &mut self.subscribers {
            if let Err(e) = subscriber.on_events(&self.events).await {
                error!("Failed to publish events to '{}': {e}", subscriber.name());
                return Err(EventLoopError::Subscriber(subscriber.name().into(), e));
            }
        }

        Ok(())
    }
}
