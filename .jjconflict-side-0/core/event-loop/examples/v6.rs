#![allow(unused_variables)]

use proton_event_loop::v6::{
    EventSource, EventSourceDependencyList, EventSubscriber, EventSubscriberResult,
};
use proton_event_loop::{
    EventId, EventProvider, EventProviderResult, RawEvent, RefreshFlag, store::EventStore,
};
use serde::Deserialize;
struct CoreEventSource;

#[derive(Debug, Deserialize)]
struct CoreEvent {
    //TODO: fill me out
}

impl EventSource for CoreEventSource {
    type Event = CoreEvent;
    type Cache = ();
    fn name() -> &'static str {
        "Core"
    }
}

struct MailEventSource;

#[derive(Debug, Deserialize)]
struct MailEvent {
    //TODO: fill me out
}

impl EventSource for MailEventSource {
    type Event = MailEvent;
    type Cache = ();
    fn name() -> &'static str {
        "Core"
    }
    fn dependencies() -> EventSourceDependencyList {
        EventSourceDependencyList::default().with::<CoreEventSource>()
    }
}

struct DummyEventProvider;

#[async_trait::async_trait]
impl EventProvider for DummyEventProvider {
    async fn get_latest_event_id(&self) -> EventProviderResult<EventId> {
        todo!()
    }
    async fn get_event(&self, event_id: &EventId) -> EventProviderResult<RawEvent> {
        todo!()
    }
}

struct DummyEventStore;

struct MailSubscriber;

#[async_trait::async_trait]
impl EventSubscriber<MailEventSource> for MailSubscriber {
    fn name(&self) -> &'static str {
        "mail_subscriber"
    }

    async fn on_event(&self, event: &MailEvent, cache: &mut ()) -> EventSubscriberResult<()> {
        todo!()
    }
    async fn on_refresh(&self, _: RefreshFlag, (): &mut ()) -> EventSubscriberResult<()> {
        todo!()
    }
}

#[async_trait::async_trait]
impl EventStore for DummyEventStore {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        todo!()
    }
    async fn store(&self, id: EventId) -> anyhow::Result<()> {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    use proton_event_loop::v6::EventManager;
    let mut manager = EventManager::new();
    manager
        .add::<CoreEventSource>(Box::new(DummyEventProvider), Box::new(DummyEventStore))
        .unwrap();
    let subscriber_id = manager.subscribe(MailSubscriber).unwrap();
    manager.poll().await.unwrap();
    manager.unsubscribe(subscriber_id);
}
