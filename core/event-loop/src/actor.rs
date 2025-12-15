use crate::store::EventStore;
use crate::subscriber::{RawSubscriber, TypedSubscribers};
use crate::{Event, EventLoopError, EventProvider, Subscriber};
use indexmap::IndexMap;
use indexmap::map::Entry;
use std::any::{Any, TypeId};
use std::pin::Pin;
use tokio::sync::{mpsc, oneshot};
use tracing::Instrument;

pub struct EventPoll {
    tx: mpsc::Sender<EventPollActorMessage>,
}

pub trait TaskSpawner {
    fn spawn(self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
}

impl<T: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>)> TaskSpawner for T {
    fn spawn(self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        self(task);
    }
}

impl EventPoll {
    #[must_use]
    pub fn new(
        task_spawner: impl TaskSpawner,
        store: Box<dyn EventStore>,
        provider: Box<dyn EventProvider>,
    ) -> Self {
        let epoll = crate::poll::EventPollInternal::new();

        // Allow some capacity for pull to refresh request to buffer up.
        let (tx, rx) = mpsc::channel(8);
        let actor = EventPollActor {
            rx,
            epoll,
            store,
            provider,
            subscribers: IndexMap::new(),
        };

        task_spawner.spawn(Box::pin(async move {
            actor.run().await;
        }));

        Self { tx }
    }

    pub async fn initialize(&self) -> Result<(), EventLoopError> {
        self.act(EventPollActorMessage::Initialize).await?
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
    epoll: crate::poll::EventPollInternal,
    store: Box<dyn EventStore>,
    provider: Box<dyn EventProvider>,
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
        for s in self.subscribers.values_mut() {
            s.cleanup();
        }

        self.epoll
            .poll_raw(
                self.store.as_ref(),
                self.provider.as_ref(),
                &self.subscribers,
                crate::poll::MAX_EVENTS_PER_POLL,
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

type RegisterFn =
    Box<dyn FnOnce(&mut IndexMap<TypeId, Box<dyn RawSubscriber>>) + Send + Sync + 'static>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockEventProvider;
    use crate::store::InMemoryEventStore;
    use crate::subscriber::SubscriberResult;
    use crate::{Event, EventId, Subscriber};
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[tokio::test]
    async fn register_same_subscriber_multiple_times() {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct FakeEvent {
            id: EventId,
        }

        impl Event for FakeEvent {
            type Response = Self;

            fn event_id(&self) -> EventId {
                self.id.clone()
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

            async fn on_events(&self, _: &mut [FakeEvent]) -> SubscriberResult<()> {
                todo!();
            }

            async fn on_refresh(&self, _: &FakeEvent) -> SubscriberResult<()> {
                todo!();
            }

            fn is_alive(&self) -> bool {
                true
            }
        }

        let target = EventPoll::new(
            |task| {
                tokio::spawn(task);
            },
            Box::new(InMemoryEventStore::default()),
            Box::new(MockEventProvider::new()),
        );

        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
        assert!(target.register(Box::new(FakeSubscriber)).await.is_ok());
    }
}
