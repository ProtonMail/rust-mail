use proton_api_mail::domain::{Label, LabelId};
use proton_api_mail::proton_api_core::exports::anyhow;
use proton_async::async_trait::async_trait;
use proton_async::tokio;
use std::collections::HashMap;

#[async_trait]
pub trait Store: Send + Sync {
    async fn read(&self) -> Box<dyn StoreReader + '_>;

    async fn write(&self) -> Box<dyn StoreWriter + '_>;
}

#[async_trait]
pub trait StoreReader: Send + Sync {
    async fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>>;
}

#[async_trait]
pub trait StoreWriter: StoreReader {
    async fn store_one(&mut self, label: &Label) -> anyhow::Result<()>;
    async fn store(&mut self, label: &[Label]) -> anyhow::Result<()>;

    async fn delete(&mut self, id: &LabelId) -> anyhow::Result<()>;

    async fn update(&mut self, label: &Label) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    map: tokio::sync::RwLock<HashMap<LabelId, Label>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
        }
    }

    pub fn with_labels(labels: impl IntoIterator<Item = Label>) -> Self {
        Self {
            map: tokio::sync::RwLock::new(HashMap::from_iter(
                labels.into_iter().map(|l| (l.id.clone(), l)),
            )),
        }
    }
}

struct MemoryStoreReader<'a>(tokio::sync::RwLockReadGuard<'a, HashMap<LabelId, Label>>);

struct MemoryStoreWriter<'a>(tokio::sync::RwLockWriteGuard<'a, HashMap<LabelId, Label>>);

// TODO: the store interface should be updated to reflect transactional storage capabilities.
#[async_trait]
impl Store for MemoryStore {
    async fn read(&self) -> Box<dyn StoreReader + '_> {
        Box::new(MemoryStoreReader(self.map.read().await))
    }

    async fn write(&self) -> Box<dyn StoreWriter + '_> {
        Box::new(MemoryStoreWriter(self.map.write().await))
    }
}

#[async_trait]
impl<'a> StoreReader for MemoryStoreReader<'a> {
    async fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>> {
        let Some(label) = self.0.get(id) else {
            return Ok(None);
        };

        Ok(Some(label.clone()))
    }
}

#[async_trait]
impl<'a> StoreReader for MemoryStoreWriter<'a> {
    async fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>> {
        let Some(label) = self.0.get(id) else {
            return Ok(None);
        };

        Ok(Some(label.clone()))
    }
}

#[async_trait]
impl<'a> StoreWriter for MemoryStoreWriter<'a> {
    async fn store_one(&mut self, label: &Label) -> anyhow::Result<()> {
        self.0.insert(label.id.clone(), label.clone());
        Ok(())
    }
    async fn store(&mut self, label: &[Label]) -> anyhow::Result<()> {
        self.0
            .extend(label.iter().map(|l| (l.id.clone(), l.clone())));
        Ok(())
    }

    async fn delete(&mut self, id: &LabelId) -> anyhow::Result<()> {
        self.0.remove(id);
        Ok(())
    }

    async fn update(&mut self, label: &Label) -> anyhow::Result<()> {
        self.0.insert(label.id.clone(), label.clone());
        Ok(())
    }
}
