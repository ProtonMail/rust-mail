use std::sync::Arc;

use arc_swap::ArcSwapOption;

mod content;
mod read;
mod write;
pub use content::*;
pub use read::*;
pub use write::*;

const COLLECTION: &str = "collection";

/// Representation of all the know documents in the current partition.
#[derive(Debug, Default)]
pub struct CollectionSansIo {
    reader: Arc<ArcSwapOption<(u64, CollectionContent)>>,
    writer: Arc<ArcSwapOption<(u64, CollectionContent)>>,
}

impl CollectionSansIo {
    pub(crate) fn len(&self) -> Option<usize> {
        self.reader.load().as_deref().map(|(_rev, col)| col.len())
    }

    pub fn reset(&self) {
        self.reader.store(None);
        self.writer.store(None);
    }
}

#[cfg(test)]
impl CollectionSansIo {
    pub fn test_new(cache: Arc<ArcSwapOption<(u64, CollectionContent)>>) -> Self {
        Self {
            reader: cache.clone(),
            writer: cache,
        }
    }
}
