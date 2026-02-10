#![allow(clippy::module_name_repetitions)]

use crate::EventId;
use async_trait::async_trait;

/// This trait allows abstraction over how to store and load events. Note that this only stores the
/// event `RemoteId`, you will need to ask the `Provider` for the actual event.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn load(&self) -> anyhow::Result<Option<EventId>>;
    async fn store(&self, id: EventId) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    id: std::sync::RwLock<Option<EventId>>,
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        let accessor = self.id.read().expect("lock poison");
        Ok(accessor.clone())
    }

    async fn store(&self, id: EventId) -> anyhow::Result<()> {
        let mut accessor = self.id.write().expect("lock poison");
        *accessor = Some(id);
        Ok(())
    }
}
