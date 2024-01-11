use proton_api_rs::domain::EventId;
use proton_api_rs::exports::anyhow;
use proton_async::async_trait::async_trait;
use proton_async::tokio;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Store: Send + Sync {
    async fn load(&self) -> anyhow::Result<Option<EventId>>;

    async fn store(&self, id: &EventId) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    id: tokio::sync::RwLock<Option<EventId>>,
}
#[async_trait]
impl Store for InMemoryStore {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        let accessor = self.id.read().await;
        Ok(accessor.clone())
    }

    async fn store(&self, id: &EventId) -> anyhow::Result<()> {
        let mut accessor = self.id.write().await;
        *accessor = Some(id.clone());
        Ok(())
    }
}
