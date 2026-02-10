use std::collections::BTreeMap;

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use crate::index::prelude::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionContent {
    /// Map of the indexes used in this partition.
    ///
    /// Those could defer depending on the updates made on the index over time and the document indexation
    attributes: IndexSet<Box<str>>,
    /// Reference from a filename to an entry index.
    entries: BTreeMap<Box<str>, EntryIndex>,
    /// Metadata of all the entries in the collection.
    identifiers: BTreeMap<EntryIndex, Box<str>>,
}
impl CollectionContent {
    pub fn remove_entry(&mut self, identifier: &str) -> Option<EntryIndex> {
        if let Some(entry) = self.entries.remove(identifier) {
            self.identifiers.remove(&entry);
            Some(entry)
        } else {
            None
        }
    }

    pub fn insert_attribute(&mut self, attr: Box<str>) -> AttributeIndex {
        let (attr, _new) = self.attributes.insert_full(attr);
        let Ok(attr) = AttributeIndex::try_from(attr) else {
            panic!("Too many attributes");
        };
        attr
    }

    /// Insert an entry into the collection.
    /// Returns Ok if newly inserted.
    /// Returns Err if already present.
    pub fn insert_entry(
        &mut self,
        identifier: Box<str>,
        batch_number: u32,
    ) -> Result<EntryIndex, EntryIndex> {
        let sequence = self.entries.len() as u32;
        let mut inserted = false;
        let entry = self.entries.entry(identifier.clone()).or_insert_with(|| {
            inserted = true;
            if batch_number == 0 {
                // Use sequential allocation for batch 0
                EntryIndex(sequence)
            } else {
                // Use Cantor pairing to generate unique EntryIndex based on batch and sequence,
                // panic on overflow
                let entry_value =
                    crate::cantor_pairing::cantor_pair_with_fallback(batch_number, sequence);
                EntryIndex(entry_value)
            }
        });
        self.identifiers.insert(*entry, identifier);

        if inserted { Ok(*entry) } else { Err(*entry) }
    }
    pub fn get_identifier(&self, entry: EntryIndex) -> Box<str> {
        #[cfg(debug_assertions)]
        if !self.identifiers.contains_key(&entry) {
            use tracing::warn;
            // If we have an EntryIndex, there must be an identifier
            // Or else we have a bug somewhere so we shall panic.
            warn!(error="missing ID", ?entry, ?self.identifiers);
        }
        self.identifiers[&entry].clone()
    }

    pub fn get_identifiers(&self) -> impl Iterator<Item = Box<str>> {
        self.identifiers.values().cloned()
    }

    pub fn get_attribute(&self, attr: &str) -> Option<AttributeIndex> {
        // An empty engine will not have attributes the app may search for
        let attr = self.attributes.get_index_of(attr)?;
        let Ok(attr) = AttributeIndex::try_from(attr) else {
            panic!("Too many attributes");
        };
        Some(attr)
    }

    pub(crate) fn get_attribute_name(&self, attr: AttributeIndex) -> &str {
        self.attributes[attr.0 as usize].as_ref()
    }

    pub fn len(&self) -> usize {
        self.identifiers.len()
    }
}
