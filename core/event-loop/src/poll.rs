use crate::store::Store;
use crate::subscriber::{RawSubscriber, TypedSubscribers};
use crate::{Event, EventLoopError, Provider, RawEvent, Subscriber};
use anyhow::anyhow;
use indexmap::{IndexMap, map::Entry};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use proton_task_service::TaskService;
use std::any::{Any, TypeId};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{self, Instrument, Level, debug, error, info};

pub struct EventPoll {
    tx: mpsc::Sender<EventPollActorMessage>,
}

impl EventPoll {
    #[must_use]
    pub fn new(
        task_service: &TaskService,
        cancellation_token: CancellationToken,
        store: Box<dyn Store>,
        provider: Box<dyn Provider>,
    ) -> Self {
        let epoll = EventPollInternal::new();

        // Allow some capacity for pull to refresh request to buffer up.
        let (tx, rx) = mpsc::channel(8);
        let actor = EventPollActor {
            rx,
            epoll,
            store,
            provider,
            subscribers: IndexMap::new(),
        };

        task_service.spawn_cancellable(cancellation_token, async move {
            actor.run().await;
        });

        Self { tx }
    }

    pub async fn initialize(&self) -> Result<&Self, EventLoopError> {
        self.act(EventPollActorMessage::Initialize).await??;
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
        self.act(|tx| EventPollActorMessage::Register {
            register: Box::new(|subscribers| match subscribers.entry(TypeId::of::<T>()) {
                Entry::Occupied(mut entry) => {
                    let entry: &mut dyn RawSubscriber = &mut **entry.get_mut();

                    if let Some(typed_subscribers) =
                        <dyn Any>::downcast_mut::<TypedSubscribers<T>>(entry)
                    {
                        typed_subscribers.add_subscriber(subscriber);
                    } else {
                        unreachable!();
                    }
                }

                Entry::Vacant(entry) => {
                    entry.insert(TypedSubscribers::<T>::new_raw(subscriber));
                }
            }),
            sender: tx,
        })
        .await?;

        Ok(self)
    }

    pub async fn poll(&self) -> Result<(), EventLoopError> {
        let span = tracing::Span::current();
        self.act(|sender| EventPollActorMessage::Poll { span, sender })
            .await?
    }

    async fn act<T: Send + 'static>(
        &self,
        closure: impl FnOnce(oneshot::Sender<T>) -> EventPollActorMessage,
    ) -> Result<T, EventLoopError> {
        let (tx, rx) = oneshot::channel();
        let msg = closure(tx);
        self.tx.send(msg).await.map_err(|_| EventLoopError::Actor)?;
        rx.await.map_err(|_| EventLoopError::Actor)
    }
}

/// Collect events from the Proton Servers in a loop and publish the events to the subscribers.
///
/// This version requires the user to call the [`EventLoop::poll`] function each time they wish to
/// iterate the loop.
#[derive(Debug, Default)]
struct EventPollInternal;

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
        let Some(last_event_id) = store.load().await.map_err(EventLoopError::StoreRead)? else {
            let e = anyhow!("No EventId in store");
            error!("{e:?}");
            return Err(EventLoopError::StoreRead(e));
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
            let new_event_id = raw_event.event_id().clone();
            has_more = raw_event.has_more();
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
            processed_event_count += 1;
        }

        Ok(())
    }

    async fn fetch_event(
        &self,
        provider: &dyn Provider,
        last_event_id: &EventId,
    ) -> Result<Option<RawEvent>, ApiServiceError> {
        let event = provider.get_event(last_event_id).await?;
        let new_event_id = event.event_id().clone();

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

type RegisterFn =
    Box<dyn FnOnce(&mut IndexMap<TypeId, Box<dyn RawSubscriber>>) + Send + Sync + 'static>;
enum EventPollActorMessage {
    Initialize(oneshot::Sender<Result<(), EventLoopError>>),
    Register {
        register: RegisterFn,
        sender: oneshot::Sender<()>,
    },
    Poll {
        span: tracing::span::Span,
        sender: oneshot::Sender<Result<(), EventLoopError>>,
    },
}

struct EventPollActor {
    rx: mpsc::Receiver<EventPollActorMessage>,
    epoll: EventPollInternal,
    store: Box<dyn Store>,
    provider: Box<dyn Provider>,
    /// The subscribers are stored in a indexmap of boxed raw subscribers.
    /// The indexmap was chosen to preserve the order of the subscribers to run - FIFO.
    /// The indexmap stores the type id of the subscriber to allow for multiple subscribers
    /// of the same type to prevent double deserialization of the same event.
    subscribers: IndexMap<TypeId, Box<dyn RawSubscriber>>,
}

impl EventPollActor {
    async fn initialize(&self) -> Result<(), EventLoopError> {
        self.epoll
            .initialize(self.store.as_ref(), self.provider.as_ref())
            .await
    }

    async fn poll(&mut self) -> Result<(), EventLoopError> {
        {
            for s in self.subscribers.values_mut() {
                s.cleanup();
            }
        }

        self.epoll
            .poll_raw(
                self.store.as_ref(),
                self.provider.as_ref(),
                &self.subscribers,
                MAX_EVENTS_PER_POLL,
            )
            .await
    }

    async fn run(mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                EventPollActorMessage::Initialize(tx) => {
                    let r = self.initialize().await;
                    let _ = tx.send(r);
                }
                EventPollActorMessage::Register { register, sender } => {
                    register(&mut self.subscribers);
                    let _ = sender.send(());
                }
                EventPollActorMessage::Poll { span, sender } => {
                    let r = self.poll().instrument(span).await;
                    let _ = sender.send(r);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventMetadata;
    use crate::provider::MockProvider;
    use crate::store::{InMemoryStore, MockStore};
    use crate::subscriber::{MockRawSubscriber, SubscriberError};
    use async_trait::async_trait;
    use mockall::predicate;
    use serde::{Deserialize, Serialize};

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

    #[tokio::test]
    async fn register_same_subscriber_multiple_times() {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct FakeEvent {
            id: EventId,
        }

        impl Event for FakeEvent {
            type Response = Self;

            fn event_id(&self) -> &EventId {
                &self.id
            }

            fn has_more(&self) -> bool {
                false
            }

            fn is_refresh(&self) -> bool {
                false
            }
        }

        #[derive(Clone, Debug)]
        struct FakeSubscriber;

        #[async_trait]
        impl Subscriber<FakeEvent> for FakeSubscriber {
            fn name(&self) -> &'static str {
                "FakeSubscriber"
            }

            async fn on_events(&self, _: &mut [FakeEvent]) -> Result<(), SubscriberError> {
                todo!();
            }

            async fn on_refresh(&self, _: &FakeEvent) -> Result<(), SubscriberError> {
                todo!();
            }

            fn is_alive(&self) -> bool {
                true
            }
        }

        let task_service = TaskService::new(tokio::runtime::Handle::current()).unwrap();
        let cancel_token = CancellationToken::new();
        let target = EventPoll::new(
            &task_service,
            cancel_token.clone(),
            Box::new(InMemoryStore::default()),
            Box::new(MockProvider::new()),
        );

        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
    }
}
