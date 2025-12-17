use crate::event_loop::EventLoopActionIds;
use proton_event_loop::store::EventStore;
use proton_event_loop::v6::{EventSource, EventSubscriber, EventSubscriberId};
use proton_event_loop::{EventLoopError, EventProvider, v6};
use proton_task_service::TaskService;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Span};

pub struct EventLoopService {
    event_poll: EventManager,
    last_event_loop_action_ids: Arc<Mutex<EventLoopActionIds>>,
}

impl EventLoopService {
    #[must_use]
    pub fn new(event_loop: EventManager) -> Self {
        Self {
            event_poll: event_loop,
            last_event_loop_action_ids: Arc::new(Mutex::new(EventLoopActionIds {
                last_event_loop_action_id_forced: None,
                last_event_loop_action_id_normal: None,
                last_rollback_action_id: None,
            })),
        }
    }

    #[must_use]
    pub fn event_poll(&self) -> &EventManager {
        &self.event_poll
    }

    #[must_use]
    pub fn last_event_loop_action_ids(&self) -> &Arc<Mutex<EventLoopActionIds>> {
        &self.last_event_loop_action_ids
    }
}

pub struct EventManager {
    tx: mpsc::Sender<EventManagerMessage>,
}

impl EventManager {
    #[must_use]
    pub fn new(task_service: &TaskService, cancellation_token: CancellationToken) -> Self {
        let (tx, rx) = mpsc::channel(2);
        let actor = EventManagerActor::new(rx);
        task_service.spawn_cancellable(cancellation_token, async move {
            actor.run().await;
        });

        Self { tx }
    }
    pub async fn add<E: EventSource>(
        &self,
        provider: Box<dyn EventProvider>,
        store: Box<dyn EventStore>,
    ) -> Result<(), EventLoopError> {
        self.run(move |manager| manager.add::<E>(provider, store))
            .await?
    }
    pub async fn subscribe<E: EventSource>(
        &self,
        subscriber: impl EventSubscriber<E>,
    ) -> Result<Option<EventSubscriberId<E>>, EventLoopError> {
        self.run(move |manager| manager.subscribe(subscriber)).await
    }

    pub async fn unsubscribe<E: EventSource>(
        &self,
        subscriber_id: EventSubscriberId<E>,
    ) -> Result<Option<Box<dyn EventSubscriber<E>>>, EventLoopError> {
        self.run(move |manager| manager.unsubscribe(subscriber_id))
            .await
    }

    pub async fn initialize(&self) -> Result<(), EventLoopError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(EventManagerMessage::Init(tx, Span::current()))
            .await
            .map_err(|_| EventLoopError::Actor)?;

        rx.await.map_err(|_| EventLoopError::Actor)?
    }

    pub async fn poll(&self) -> Result<(), EventLoopError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(EventManagerMessage::Poll(tx, Span::current()))
            .await
            .map_err(|_| EventLoopError::Actor)?;

        rx.await.map_err(|_| EventLoopError::Actor)?
    }

    async fn run<F, T>(&self, closure: F) -> Result<T, EventLoopError>
    where
        F: FnOnce(&mut v6::EventManager) -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(EventManagerMessage::Run(Box::new(move |manager| {
                let r = closure(manager);
                let _ = tx.send(r);
            })))
            .await
            .map_err(|_| EventLoopError::Actor)?;

        rx.await.map_err(|_| EventLoopError::Actor)
    }
}

enum EventManagerMessage {
    Run(Box<dyn FnOnce(&mut v6::EventManager) + Send + 'static>),
    Poll(oneshot::Sender<Result<(), EventLoopError>>, Span),
    Init(oneshot::Sender<Result<(), EventLoopError>>, Span),
}
struct EventManagerActor {
    rx: mpsc::Receiver<EventManagerMessage>,
    manager: v6::EventManager,
}

impl EventManagerActor {
    fn new(rx: mpsc::Receiver<EventManagerMessage>) -> Self {
        Self {
            rx,
            manager: v6::EventManager::new(),
        }
    }

    async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            match event {
                EventManagerMessage::Run(f) => f(&mut self.manager),
                EventManagerMessage::Poll(sender, span) => {
                    let r = self.manager.poll().instrument(span).await;
                    let _ = sender.send(r);
                }
                EventManagerMessage::Init(sender, span) => {
                    let r = self.manager.initialize_all().instrument(span).await;
                    let _ = sender.send(r);
                }
            }
        }
    }
}
