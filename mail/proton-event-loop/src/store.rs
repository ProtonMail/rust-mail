use proton_api_core::domain::EventId;
use proton_api_core::exports::anyhow;
use proton_async::async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
pub trait Store: Send + Sync {
    fn load(&self) -> anyhow::Result<Option<EventId>>;

    fn store(&self, id: &EventId) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    id: std::sync::RwLock<Option<EventId>>,
}
#[async_trait]
impl Store for InMemoryStore {
    fn load(&self) -> anyhow::Result<Option<EventId>> {
        let accessor = self.id.read().expect("lock poison");
        Ok(accessor.clone())
    }

    fn store(&self, id: &EventId) -> anyhow::Result<()> {
        let mut accessor = self.id.write().expect("lock poison");
        *accessor = Some(id.clone());
        Ok(())
    }
}
