#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use proton_api_core::services::proton::common::RemoteId;

/// This trait allows abstraction over how to store and load events. Note that this only stores the
/// event RemoteId, you will need to ask the `Provider` for the actual event.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Store: Send + Sync {
    /// Load the latest event id from the store.
    ///
    /// # Errors
    /// Returns error if value failed to be loaded.
    async fn load(&self) -> anyhow::Result<Option<RemoteId>>;

    /// Store the latest event id into the store.
    ///
    /// # Errors
    /// Returns error if value failed to be stored.
    ///
    async fn store(&self, id: RemoteId) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    id: std::sync::RwLock<Option<RemoteId>>,
}
#[async_trait]
impl Store for InMemoryStore {
    async fn load(&self) -> anyhow::Result<Option<RemoteId>> {
        let accessor = self.id.read().expect("lock poison");
        Ok(accessor.clone())
    }

    async fn store(&self, id: RemoteId) -> anyhow::Result<()> {
        let mut accessor = self.id.write().expect("lock poison");
        *accessor = Some(id);
        Ok(())
    }
}
