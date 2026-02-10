use std::collections::BTreeMap;
use std::error::Error;

use proton_foundation_search::engine::{CleanupEvent, QueryEvent, WriteEvent};
use proton_foundation_search::query::results::FoundEntry;
use proton_foundation_search::query::stats::CollectionStats;
use proton_foundation_search::serialization::SerDes;
use proton_foundation_search::transaction::{LoadEvent, SaveEvent};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Storage {
    serdes: SerDes,
    blobs: BTreeMap<Box<str>, Vec<u8>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Modified(Box<str>);

impl Storage {
    pub fn handle_search(
        &mut self,
        search: impl Iterator<Item = QueryEvent>,
    ) -> impl Iterator<Item = Result<FoundEntry, Box<dyn Error + Send + Sync>>> {
        let mut stats = CollectionStats::default();
        let mut found = search
            .filter_map(|event| match event {
                QueryEvent::Load(load) => self.handle_load(load),
                QueryEvent::Found(found) => Some(Ok(found)),
                QueryEvent::Stats(collection_stats) => {
                    stats += collection_stats;
                    None
                }
            })
            .collect::<Vec<_>>();
        stats.update_all_scores(found.iter_mut().filter_map(|result| result.as_mut().ok()));
        found.into_iter()
    }

    pub fn handle_write(
        &mut self,
        write: impl Iterator<Item = WriteEvent>,
    ) -> impl Iterator<Item = Result<Modified, Box<dyn Error + Send + Sync>>> {
        write.filter_map(|event| match event {
            WriteEvent::Modified(identifier) => Some(Ok(Modified(identifier))),
            WriteEvent::Load(load) => self.handle_load(load),
            WriteEvent::Save(save) => self.handle_save(save),
        })
    }

    #[allow(dead_code)]
    pub fn handle_cleanup(
        &mut self,
        cleanup: impl Iterator<Item = CleanupEvent>,
    ) -> impl Iterator<Item = Result<Modified, Box<dyn Error + Send + Sync>>> {
        cleanup.filter_map(|event| match event {
            CleanupEvent::Release(name) => {
                self.blobs.remove(&name);
                None
            }
            CleanupEvent::Load(load) => self.handle_load(load),
            CleanupEvent::Save(save) => self.handle_save(save),
        })
    }

    fn handle_load<T>(
        &mut self,
        load: LoadEvent,
    ) -> Option<Result<T, Box<dyn Error + Send + Sync>>> {
        let LoadEvent { name, send } = load;
        if let Err(err) = send(
            &self.serdes,
            self.blobs.get(&name).cloned().unwrap_or_default(),
        ) {
            Some(Err(err))
        } else {
            None
        }
    }

    fn handle_save<T>(
        &mut self,
        save: SaveEvent,
    ) -> Option<Result<T, Box<dyn Error + Send + Sync>>> {
        let SaveEvent { name, recv } = save;
        match recv(&self.serdes) {
            Err(err) => Some(Err(err)),
            Ok(blob) => {
                self.blobs.insert(name, blob);
                None
            }
        }
    }
}
