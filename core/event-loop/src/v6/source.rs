use serde::Deserialize;
use std::any::TypeId;
use std::fmt::Debug;

/// Defines a source of events.
///
/// While this does not provide any concrete implementation about where the [`EventId`] should be
/// stored or how it should be fetched, it does define other relevant things.
pub trait EventSource: Send + Sync + 'static {
    /// Typed data which will be deserialized into after being fetched from the [`Provider`]
    /// and passed to the [`Subscriber`].
    type Event: Debug + for<'de> Deserialize<'de> + Send + Sync;

    /// With v6, it's expected that the subscribers fetch all the data they need from the server.
    /// It's possible multiple subscribers for the same source may want to fetch the same data.
    /// This type allows one to share previously fetched sources with subsequent subscribers to
    /// avoid re-fetches.
    type Cache: Default + Send;

    fn name() -> &'static str;

    /// If this event source depends on other event loops to run first, specify them in
    /// via the returned [`EventSourceDependencyList`]
    #[must_use]
    fn dependencies() -> EventSourceDependencyList {
        EventSourceDependencyList::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventSourceDependencyList(Vec<TypeId>);

impl EventSourceDependencyList {
    #[must_use]
    pub fn with<E: EventSource>(mut self) -> Self {
        self.0.push(TypeId::of::<E>());
        self
    }

    pub(crate) fn into_inner(self) -> Vec<TypeId> {
        self.0
    }
}
