use crate::store::EventStore;
use crate::v6::poller::{EventPoller, MAX_EVENTS_PER_POLL};
use crate::v6::source::EventSource;
use crate::v6::subscriber::{
    EventSubscriber, EventSubscriberId, SubscriberList, TypedSubscriberList,
};
use crate::{EventLoopError, EventProvider, RefreshFlag};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use topological_sort::TopologicalSort;
use tracing::{Instrument, instrument};

/// Manages multiples [`EventSources`] and their subscribers. It also ensures that
/// the [`EventSources`] are scheduled in the correct order.
///
/// # Initialization
///
/// [`EventSources`] are not explicitly initialized since we can't foresee all different
/// ways this may be set up by the integrating applications. You can either initialize them
/// all together with [`initialize_all()`] or individually with [`initialize`].
///
/// # Polling
///
/// To run one iteration of event polling, call the [`poll()`] method, this will call one
/// iteration of the event sources in an order that satisfies their dependencies. Execution
/// order remains stable until sources are added or removed.
///
/// There is no parallel execution of the loops and each loop iteration waits for the fetched
/// event to be applied successfully to each subscriber.
///
/// # Error Handling
///
/// Every event will be attempted 3 times if the error returned from the [`EventSubscriber`] is retryable.
/// After 3 attempts, the process is interrupted and the error bubbles up to the [`poll()`] site.
///
/// It's up to integrators to decide how to best handle this failure case.
///

#[derive(Default)]
pub struct EventManager {
    sources: HashMap<TypeId, EventSourceData>,
    exec_order: Vec<TypeId>,
}

impl EventManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            exec_order: Vec::new(),
        }
    }

    pub fn add<E: EventSource>(
        &mut self,
        provider: Box<dyn EventProvider>,
        store: Box<dyn EventStore>,
    ) -> Result<(), EventLoopError> {
        match self.sources.entry(TypeId::of::<E>()) {
            Entry::Occupied(_) => return Err(EventLoopError::DuplicateEventSource(E::name())),
            Entry::Vacant(v) => {
                tracing::info!("Adding event source ({})", E::name());
                v.insert(EventSourceData::new::<E>(provider, store));
            }
        }

        let r = self.rebuild_execution_order();
        if matches!(&r, Err(EventLoopError::CyclicDependency)) {
            self.remove::<E>();
        }
        r
    }

    fn rebuild_execution_order(&mut self) -> Result<(), EventLoopError> {
        let mut sorter = TopologicalSort::new();
        self.exec_order.clear();

        for (source_id, source) in &self.sources {
            sorter.insert(*source_id);
            for id in &source.dependencies {
                sorter.add_dependency(*id, *source_id);
            }
        }

        match sorter.pop() {
            Some(id) => {
                self.exec_order.push(id);
            }
            None if !sorter.is_empty() => return Err(EventLoopError::CyclicDependency),
            None => return Ok(()),
        }

        while let Some(id) = sorter.pop() {
            self.exec_order.push(id);
        }

        Ok(())
    }

    pub fn remove<E: EventSource>(&mut self) {
        self.sources.remove(&TypeId::of::<E>()).inspect(|s| {
            tracing::info!("Removed event source ({})", s.name);
        });
        self.rebuild_execution_order().expect("Should never fail");
    }

    pub fn subscribe<E: EventSource>(
        &mut self,
        subscriber: impl EventSubscriber<E>,
    ) -> Option<EventSubscriberId<E>> {
        let source = self.sources.get_mut(&TypeId::of::<E>())?;
        let type_list =
            <dyn Any>::downcast_mut::<TypedSubscriberList<E>>(source.subscribers.as_mut())?;
        type_list.add(Box::new(subscriber))
    }

    pub fn unsubscribe<E: EventSource>(
        &mut self,
        id: EventSubscriberId<E>,
    ) -> Option<Box<dyn EventSubscriber<E>>> {
        let source = self.sources.get_mut(&TypeId::of::<E>())?;
        let type_list =
            <dyn Any>::downcast_mut::<TypedSubscriberList<E>>(source.subscribers.as_mut())?;
        type_list
            .remove(id)
            .inspect(|s| tracing::info!("Removing event subscriber ({})", s.name()))
    }

    #[instrument(skip_all)]
    pub async fn initialize_all(&mut self) -> Result<(), EventLoopError> {
        for id in &self.exec_order {
            if let Some(source) = self.sources.get_mut(id) {
                let name = source.name;
                EventPoller::new()
                    .initialize(source.store.as_ref(), source.provider.as_ref())
                    .instrument(tracing::debug_span!("Initializing event loop", name=?name))
                    .await?;
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn initialize<E: EventSource>(&mut self) -> Result<(), EventLoopError> {
        if let Some(source) = self.sources.get_mut(&TypeId::of::<E>()) {
            let name = source.name;
            EventPoller::new()
                .initialize(source.store.as_ref(), source.provider.as_ref())
                .instrument(tracing::debug_span!("Initializing event loop", name=?name))
                .await?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn poll(&mut self) -> Result<(), EventLoopError> {
        if self.exec_order.is_empty() {
            tracing::warn!("No event sources registered");
            return Ok(());
        }

        for id in &self.exec_order {
            self.poll_source(*id).await?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn refresh(&mut self) -> Result<(), EventLoopError> {
        if self.exec_order.is_empty() {
            tracing::warn!("No event sources registered");
            return Ok(());
        }

        for id in &self.exec_order {
            self.refresh_source(*id).await?;
        }

        Ok(())
    }

    async fn poll_source(&self, type_id: TypeId) -> Result<(), EventLoopError> {
        let Some(source) = self.sources.get(&type_id) else {
            return Ok(());
        };

        EventPoller::new()
            .poll(
                source.store.as_ref(),
                source.provider.as_ref(),
                source.subscribers.as_ref(),
                MAX_EVENTS_PER_POLL,
            )
            .instrument(tracing::debug_span!("Polling", name=?source.name))
            .await
    }

    async fn refresh_source(&self, type_id: TypeId) -> Result<(), EventLoopError> {
        let Some(source) = self.sources.get(&type_id) else {
            return Ok(());
        };
        source
            .subscribers
            .on_refresh(RefreshFlag::from(true))
            .instrument(tracing::debug_span!("Refreshing", name=?source.name))
            .await
    }
}

struct EventSourceData {
    name: &'static str,
    provider: Box<dyn EventProvider>,
    store: Box<dyn EventStore>,
    subscribers: Box<dyn SubscriberList>,
    dependencies: Vec<TypeId>,
}

impl EventSourceData {
    fn new<E: EventSource>(provider: Box<dyn EventProvider>, store: Box<dyn EventStore>) -> Self {
        Self {
            name: E::name(),
            provider,
            store,
            subscribers: TypedSubscriberList::<E>::default().boxed(),
            dependencies: E::dependencies().into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockEventProvider;
    use crate::store::MockEventStore;
    use crate::v6::source::EventSourceDependencyList;
    use crate::v6::subscriber::{EventSubscriberError, MockEventSubscriber};
    use crate::{EventId, EventMetadata, RawEvent};
    use mockall::Sequence;
    use serde_with::serde_derive::Deserialize;

    #[derive(Deserialize, Eq, PartialEq, Debug, Clone)]
    struct TestEvent {}

    struct TestEventSourceA;

    impl EventSource for TestEventSourceA {
        type Event = TestEvent;
        type Cache = ();

        fn name() -> &'static str {
            "TestEventSourceA"
        }

        fn dependencies() -> EventSourceDependencyList {
            EventSourceDependencyList::default()
        }
    }
    struct TestEventSourceB;

    impl EventSource for TestEventSourceB {
        type Event = TestEvent;

        type Cache = ();

        fn name() -> &'static str {
            "TestEventSourceB"
        }

        fn dependencies() -> EventSourceDependencyList {
            EventSourceDependencyList::default().with::<TestEventSourceA>()
        }
    }

    struct TestEventSourceC;

    impl EventSource for TestEventSourceC {
        type Event = TestEvent;

        type Cache = ();

        fn name() -> &'static str {
            "TestEventSourceC"
        }

        fn dependencies() -> EventSourceDependencyList {
            EventSourceDependencyList::default()
                .with::<TestEventSourceA>()
                .with::<TestEventSourceB>()
        }
    }

    struct TestEventSourceD;
    struct TestEventSourceE;

    impl EventSource for TestEventSourceD {
        type Event = TestEvent;

        type Cache = ();

        fn name() -> &'static str {
            "TestEventSourceD"
        }

        fn dependencies() -> EventSourceDependencyList {
            EventSourceDependencyList::default().with::<TestEventSourceE>()
        }
    }

    impl EventSource for TestEventSourceE {
        type Event = TestEvent;

        type Cache = ();

        fn name() -> &'static str {
            "TestEventSourceE"
        }

        fn dependencies() -> EventSourceDependencyList {
            EventSourceDependencyList::default().with::<TestEventSourceD>()
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("TestError")]
    struct TestSubscriberError {
        is_network: bool,
        is_retryable: bool,
    }

    impl EventSubscriberError for TestSubscriberError {
        fn is_network_failure(&self) -> bool {
            self.is_network
        }

        fn is_retryable(&self) -> bool {
            self.is_retryable
        }
    }

    #[tokio::test]
    async fn poll_order() {
        let event_id_a = EventId::from("A");
        let event_id_b = EventId::from("B");
        let event_id_c = EventId::from("C");

        let mut sequence = Sequence::new();

        let mut mk_sequence = |id: EventId| -> (Box<dyn EventProvider>, Box<dyn EventStore>) {
            make_success_sequence(&id.clone(), id, &mut sequence)
        };

        let (provider_a, store_a) = mk_sequence(event_id_a);
        let (provider_b, store_b) = mk_sequence(event_id_b);
        let (provider_c, store_c) = mk_sequence(event_id_c);

        let mut manager = EventManager::new();

        manager
            .add::<TestEventSourceA>(provider_a, store_a)
            .unwrap();
        manager
            .add::<TestEventSourceB>(provider_b, store_b)
            .unwrap();
        manager
            .add::<TestEventSourceC>(provider_c, store_c)
            .unwrap();

        manager.poll().await.unwrap();
    }

    #[tokio::test]
    async fn cyclic_dependencies() {
        let mut manager = EventManager::new();

        manager
            .add::<TestEventSourceD>(
                Box::new(MockEventProvider::new()),
                Box::new(MockEventStore::new()),
            )
            .unwrap();
        let err = manager
            .add::<TestEventSourceE>(
                Box::new(MockEventProvider::new()),
                Box::new(MockEventStore::new()),
            )
            .unwrap_err();
        assert!(matches!(err, EventLoopError::CyclicDependency));
    }

    #[tokio::test]
    async fn subscriber_retried_3_on_event_failure() {
        let mut sequence = Sequence::new();
        let (provider, store) =
            make_success_sequence(&EventId::from("A"), EventId::from("B"), &mut sequence);
        let mut manager = EventManager::new();

        manager.add::<TestEventSourceA>(provider, store).unwrap();

        let mut subscriber_success = MockEventSubscriber::<TestEventSourceA>::new();
        let mut subscriber_failure = MockEventSubscriber::<TestEventSourceA>::new();

        subscriber_success.expect_name().return_const("A");
        subscriber_success
            .expect_on_event()
            .once()
            .returning(|_, ()| Ok(()));
        subscriber_failure.expect_name().return_const("B");
        subscriber_failure
            .expect_on_event()
            .times(3)
            .returning(|_, ()| {
                Err(Box::new(TestSubscriberError {
                    is_retryable: true,
                    is_network: false,
                }))
            });

        manager.subscribe(subscriber_success).unwrap();
        manager.subscribe(subscriber_failure).unwrap();

        manager.poll().await.unwrap_err();
    }

    #[tokio::test]
    async fn subscriber_retried_3_on_refresh_failure() {
        let mut sequence = Sequence::new();
        let (provider, store) = make_success_sequence_with_refresh(
            &EventId::from("A"),
            EventId::from("B"),
            &mut sequence,
            255,
        );
        let mut manager = EventManager::new();

        manager.add::<TestEventSourceA>(provider, store).unwrap();

        let mut subscriber_success = MockEventSubscriber::<TestEventSourceA>::new();
        let mut subscriber_failure = MockEventSubscriber::<TestEventSourceA>::new();

        subscriber_success.expect_name().return_const("A");
        subscriber_success
            .expect_on_refresh()
            .once()
            .returning(|_, ()| Ok(()));
        subscriber_failure.expect_name().return_const("B");
        subscriber_failure
            .expect_on_refresh()
            .times(3)
            .returning(|_, ()| {
                Err(Box::new(TestSubscriberError {
                    is_retryable: true,
                    is_network: false,
                }))
            });

        manager.subscribe(subscriber_success).unwrap();
        manager.subscribe(subscriber_failure).unwrap();

        manager.poll().await.unwrap_err();
    }

    #[tokio::test]
    async fn duplicate_subscriber_or_event_source() {
        let mut manager = EventManager::new();

        manager
            .add::<TestEventSourceA>(
                Box::new(MockEventProvider::new()),
                Box::new(MockEventStore::new()),
            )
            .unwrap();
        let err = manager
            .add::<TestEventSourceA>(
                Box::new(MockEventProvider::new()),
                Box::new(MockEventStore::new()),
            )
            .unwrap_err();
        assert!(matches!(err, EventLoopError::DuplicateEventSource(_)));

        let mut subscriber = MockEventSubscriber::<TestEventSourceA>::new();
        subscriber.expect_name().return_const("A");
        manager.subscribe(subscriber).unwrap();

        let mut subscriber = MockEventSubscriber::<TestEventSourceA>::new();
        subscriber.expect_name().return_const("A");
        assert!(manager.subscribe(subscriber).is_none());
    }

    fn make_success_sequence(
        id: &EventId,
        next_event_id: EventId,
        sequence: &mut Sequence,
    ) -> (Box<dyn EventProvider>, Box<dyn EventStore>) {
        make_success_sequence_with_refresh(id, next_event_id, sequence, 0)
    }

    fn make_success_sequence_with_refresh(
        id: &EventId,
        next_event_id: EventId,
        sequence: &mut Sequence,
        refresh: u8,
    ) -> (Box<dyn EventProvider>, Box<dyn EventStore>) {
        let mut store = MockEventStore::new();
        let mut provider = MockEventProvider::new();
        let event_id = id.clone();
        store
            .expect_load()
            .once()
            .in_sequence(sequence)
            .returning(move || Ok(Some(event_id.clone())));
        provider
            .expect_get_event()
            .once()
            .in_sequence(sequence)
            .returning(move |_| {
                Ok(RawEvent {
                    meta: EventMetadata {
                        event_id: next_event_id.clone(),
                        has_more: 0.into(),
                        refresh: refresh.into(),
                    },
                    raw: "{}".into(),
                })
            });

        (Box::new(provider), Box::new(store))
    }
}
