//! Generic event service applications can use to subscribe or publish typed event data.
//!
//! Events need to be registered before they can be published or subscribed to.
//!
//! # Example
//!
//! ```
//! use proton_event_service::{Event, EventService};
//!
//! #[derive(Debug, Clone, Eq, PartialEq,Copy)]
//! enum FooEvent {
//!     FooCreated,
//!     FooDeleted,
//!}
//!
//! async fn events() {
//!    let mut event_service = EventService::new();
//!    event_service.register::<FooEvent>();
//!
//!    let mut stream = event_service.subscribe::<FooEvent>().unwrap();
//!
//!    event_service.publish(FooEvent::FooCreated);
//!
//!    let event = stream.next().await.unwrap();
//!    assert_eq!(event, FooEvent::FooCreated);
//!}
//!
//!
//!
//! ```
//!
use parking_lot::RwLock;
use std::any::{Any, TypeId};
use std::collections::HashMap;

pub trait Event: Send + Sync + Clone + 'static {}
impl<T: Send + Sync + Clone + 'static> Event for T {}

#[derive(Default)]
pub struct EventService {
    event_listeners: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync + 'static>>>,
}

impl EventService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            event_listeners: RwLock::new(HashMap::new()),
        }
    }

    /// This method will create an event stream where 1 event can be stored in the stream at any
    /// given time.
    ///
    /// Use [`register_with_capacity`] if you need more than one event in the stream.
    pub fn register<T: Event>(&self) {
        self.register_with_capacity::<T>(1);
    }

    /// This method will create an event stream where up to `capacity` events can remain in the
    /// stream at any given time.
    pub fn register_with_capacity<T: Event>(&self, capacity: usize) {
        let mut listeners = self.event_listeners.write();
        listeners
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(EventListener::<T>::new(capacity)));
    }
    pub fn unregister<T: Event>(&self) {
        let mut listeners = self.event_listeners.write();
        listeners.remove(&TypeId::of::<T>());
    }

    #[must_use]
    pub fn subscribe<T: Event>(&self) -> Option<EventStream<T>> {
        let listeners = self.event_listeners.read();
        let listener = listeners.get(&TypeId::of::<T>())?;
        listener
            .downcast_ref::<EventListener<T>>()
            .map(EventListener::new_stream)
    }

    pub fn publish<T: Event>(&self, event: T) {
        let listeners = self.event_listeners.read();
        let Some(listener) = listeners.get(&TypeId::of::<T>()) else {
            tracing::warn!("No event registered for {}", std::any::type_name::<T>());
            return;
        };
        let Some(listener) = listener.downcast_ref::<EventListener<T>>() else {
            unreachable!(
                "Failed to downcasts listener to {}",
                std::any::type_name::<T>()
            );
        };

        // never fails since we always keep on receiver alive
        let _ = listener.sender.send(event);
    }
}

#[derive(Debug)]
pub struct EventStreamTerminated;

pub struct EventStream<T: Event>(tokio::sync::broadcast::Receiver<T>);

impl<T: Event> EventStream<T> {
    pub async fn next(&mut self) -> Result<T, EventStreamTerminated> {
        loop {
            match self.0.recv().await {
                Ok(event) => return Ok(event),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(EventStreamTerminated);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // do nothing
                }
            }
        }
    }
}

struct EventListener<T: Event> {
    sender: tokio::sync::broadcast::Sender<T>,
}

impl<T: Event> EventListener<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender: tx }
    }

    fn new_stream(&self) -> EventStream<T> {
        EventStream(self.sender.subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Eq, PartialEq, Copy)]
    enum FooEvent {
        FooCreated,
    }

    #[tokio::test]
    async fn basic() {
        let event_service = EventService::new();
        event_service.register::<FooEvent>();
        let mut stream = event_service.subscribe::<FooEvent>().unwrap();

        event_service.publish(FooEvent::FooCreated);

        let event = stream.next().await.unwrap();
        assert_eq!(event, FooEvent::FooCreated);
    }

    #[tokio::test]
    async fn subscribe_without_register() {
        let event_service = EventService::new();
        assert!(event_service.subscribe::<FooEvent>().is_none());
    }
}
