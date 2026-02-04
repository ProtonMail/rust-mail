mod executor;
#[cfg(test)]
mod tests;

use alloc::collections::{BTreeMap, BTreeSet};
use core::ops::ControlFlow;

use self::executor::Executor;
use super::*;
use crate::blob::{BlobId, LoadEvent};
use crate::index::prelude::*;
use crate::prelude::*;

/// Write staging buffer, not making any changes to the index yet, just preparing
#[derive(Debug)]
pub struct Write {
    id: Option<BlobId>,
    staging: Staging,
    index: TextIndex,
}

// Write staging buffers
#[derive(Debug, Default)]
struct Staging {
    removals: Vec<(EntryIndex, AttributeIndex, AttributeIndex)>,
    inserts: BTreeMap<(AttributeIndex, EntryIndex), Arc<EntryValues>>,
}

impl Write {
    pub fn new(index: TextIndex, id: Option<BlobId>) -> Self {
        Self {
            id,
            staging: Default::default(),
            index,
        }
    }

    /// Ideally, the app would remove first and then insert for optimal performance
    /// but we can make no such assumptions here. Maybe the app inserts a new entry
    /// only to delete it immediately. Therefore, removals affect staged inserts as well.
    pub fn remove(&mut self, entry: EntryIndex) {
        self.staging
            .removals
            .push((entry, AttributeIndex(0), AttributeIndex(u32::MAX)));
        self.staging.inserts.retain(|(_a, e), _| *e != entry);
    }

    /// To insert cleanly, we do have to remove any current tokens and trigrams to maintain integrity.
    pub fn insert(&mut self, entry: EntryIndex, attr: AttributeIndex, values: Arc<EntryValues>) {
        // we shall remove existing content first
        self.staging.removals.push((entry, attr, attr));
        // stage the inserts
        self.staging.inserts.insert((attr, entry), values);
    }

    pub fn commit(self) -> Executor {
        Executor::new(self)
    }
}
