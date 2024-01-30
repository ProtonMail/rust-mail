use proton_api_mail::domain::{Label, LabelId};
use proton_api_mail::proton_api_core::exports::{anyhow, parking_lot};
use std::collections::HashMap;

pub trait Store: Send + Sync {
    fn read(&self) -> Box<dyn StoreReader + '_>;

    fn write(&self) -> Box<dyn StoreWriter + '_>;
}

pub trait StoreReader {
    fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>>;
}

pub trait StoreWriter: StoreReader {
    fn store_one(&mut self, label: &Label) -> anyhow::Result<()>;
    fn store(&mut self, label: &[Label]) -> anyhow::Result<()>;

    fn delete(&mut self, id: &LabelId) -> anyhow::Result<()>;

    fn update(&mut self, label: &Label) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    map: parking_lot::RwLock<HashMap<LabelId, Label>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
        }
    }

    pub fn with_labels(labels: impl IntoIterator<Item = Label>) -> Self {
        Self {
            map: parking_lot::RwLock::new(HashMap::from_iter(
                labels.into_iter().map(|l| (l.id.clone(), l)),
            )),
        }
    }
}

struct MemoryStoreReader<'a>(parking_lot::RwLockReadGuard<'a, HashMap<LabelId, Label>>);

struct MemoryStoreWriter<'a>(parking_lot::RwLockWriteGuard<'a, HashMap<LabelId, Label>>);

// TODO: the store interface should be updated to reflect transactional storage capabilities.
impl Store for MemoryStore {
    fn read(&self) -> Box<dyn StoreReader + '_> {
        Box::new(MemoryStoreReader(self.map.read()))
    }

    fn write(&self) -> Box<dyn StoreWriter + '_> {
        Box::new(MemoryStoreWriter(self.map.write()))
    }
}

impl<'a> StoreReader for MemoryStoreReader<'a> {
    fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>> {
        let Some(label) = self.0.get(id) else {
            return Ok(None);
        };

        Ok(Some(label.clone()))
    }
}

impl<'a> StoreReader for MemoryStoreWriter<'a> {
    fn get(&self, id: &LabelId) -> anyhow::Result<Option<Label>> {
        let Some(label) = self.0.get(id) else {
            return Ok(None);
        };

        Ok(Some(label.clone()))
    }
}

impl<'a> StoreWriter for MemoryStoreWriter<'a> {
    fn store_one(&mut self, label: &Label) -> anyhow::Result<()> {
        self.0.insert(label.id.clone(), label.clone());
        Ok(())
    }
    fn store(&mut self, label: &[Label]) -> anyhow::Result<()> {
        self.0
            .extend(label.iter().map(|l| (l.id.clone(), l.clone())));
        Ok(())
    }

    fn delete(&mut self, id: &LabelId) -> anyhow::Result<()> {
        self.0.remove(id);
        Ok(())
    }

    fn update(&mut self, label: &Label) -> anyhow::Result<()> {
        self.0.insert(label.id.clone(), label.clone());
        Ok(())
    }
}
