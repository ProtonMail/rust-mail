use std::collections::BTreeMap;
use std::hash::Hash;

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
    pub fn insert_entry(&mut self, identifier: Box<str>) -> Result<EntryIndex, EntryIndex> {
        let mut inserted = false;
        let entry = self.entries.entry(identifier.clone()).or_insert_with(|| {
            inserted = true;
            let length = self.identifiers.len() as u32;
            let sequence =
                if self.identifiers.last_key_value().map(|(k, _)| k.0 + 1) == Some(length) {
                    // compact, no gaps, the next key follows
                    length
                } else {
                    fill_gap(self.identifiers.keys().map(|e| e.0))
                };
            EntryIndex(sequence)
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

    /// Get the EntryIndex for an identifier, if it exists.
    pub fn get_entries(&self) -> impl Iterator<Item = EntryIndex> {
        self.identifiers.keys().copied()
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

impl Hash for CollectionContent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            attributes,
            entries,
            identifiers,
        } = self;
        attributes
            .iter()
            .enumerate()
            .collect::<Vec<_>>()
            .hash(state);
        entries.hash(state);
        identifiers.hash(state);
    }
}

trait Num:
    core::ops::Add<Output = Self> + core::ops::Sub<Output = Self> + PartialEq + Default + Copy
{
    const ONE: Self;
}
impl Num for u32 {
    const ONE: Self = 1;
}
impl Num for u8 {
    const ONE: Self = 1;
}

// find the next key in a sequence, filling the gaps
fn fill_gap<N, I>(list: I) -> N
where
    N: Num,
    I: Iterator<Item = N>,
{
    let i0 = N::default();
    let i1 = N::ONE;
    let mut last = None;
    list.map(|key| (last.replace(key), key))
        .filter_map(|(a, b)| {
            match a {
                // first entry gap
                None if b != i0 => Some(i0),
                // subsequent pairs with gaps
                Some(a) if b - a != i1 => Some(a + i1),
                // no gap
                _ => None,
            }
        })
        .next()
        .unwrap_or_else(|| last.map(|key| key + i1).unwrap_or_default())
}
