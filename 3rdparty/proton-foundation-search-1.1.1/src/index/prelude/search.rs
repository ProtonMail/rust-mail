//! Index searches.

use std::collections::BTreeMap;
use std::ops::AddAssign;

use crate::blob::{BlobId, LoadEvent};
use crate::document::Value;
use crate::index::prelude::*;
use crate::query::expression::{Func, TermValue};
use crate::query::option::QueryOptions;
use crate::query::results::Score;

/// Index filter
pub trait IndexSearch {
    /// Search the index at revision for a value by function and optional attribute
    ///
    /// The engine requires results from individual indices to be ordered by [`EntryIndex`].
    /// If not sorted, query expression condition will return invalid results as they are optimized with sorted merge.
    fn search(
        &self,
        revision: BlobId,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &TermValue,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>>;
}

/// Index level search event
#[derive(Debug)]
pub enum IndexSearchEvent {
    /// The search requires index content
    Load(LoadEvent),
    /// The search matched and scored an entry
    Found(EntryIndex, Vec<MatchedIndexTerm>),
    /// Search statistics
    Stats(IndexSearchStats),
}

/// An attribute value that matches a search term
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchedIndexTerm {
    /// matched value
    pub value: Value,
    /// how well did the term match in the range 0-1 with 1 being an exact match
    pub score: Score,
    /// where were the values found (value_index, token_position)
    pub positions: Vec<(AttributeIndex, ValueIndex, TokenPosition)>,
}

/// Search statistics from the index
#[derive(Debug, Default)]
pub struct IndexSearchStats {
    stats: BTreeMap<AttributeIndex, IndexSearchAttributeStats>,
}

impl IntoIterator for IndexSearchStats {
    type Item = (AttributeIndex, IndexSearchAttributeStats);

    type IntoIter =
        std::collections::btree_map::IntoIter<AttributeIndex, IndexSearchAttributeStats>;

    fn into_iter(self) -> Self::IntoIter {
        self.stats.into_iter()
    }
}

impl<'a> IntoIterator for &'a IndexSearchStats {
    type Item = (&'a AttributeIndex, &'a IndexSearchAttributeStats);

    type IntoIter =
        std::collections::btree_map::Iter<'a, AttributeIndex, IndexSearchAttributeStats>;

    fn into_iter(self) -> Self::IntoIter {
        self.stats.iter()
    }
}

impl FromIterator<(AttributeIndex, IndexSearchAttributeStats)> for IndexSearchStats {
    fn from_iter<T: IntoIterator<Item = (AttributeIndex, IndexSearchAttributeStats)>>(
        iter: T,
    ) -> Self {
        Self {
            stats: iter.into_iter().collect(),
        }
    }
}

impl IndexSearchStats {
    /// Update stats with a term match.
    pub fn matched(&mut self, entry: EntryIndex, matched: &MatchedIndexTerm) {
        for (attribute, ..) in &matched.positions {
            // update attribute frequencies and entry sizes
            let stat = self
                .stats
                .entry(*attribute)
                .or_insert_with(|| IndexSearchAttributeStats {
                    frequencies: BTreeMap::new(),
                    sizes: BTreeMap::new(),
                });

            stat.frequencies
                .entry(matched.value.clone())
                .or_default()
                .add_assign(1);

            let (_size, matches) = stat.sizes.entry(entry).or_default();
            *matches = true;
        }
    }

    /// Increment the number of tokens found per attribute and entry
    pub fn increment_size(&mut self, attribute: AttributeIndex, entry: EntryIndex, count: usize) {
        let stat = self
            .stats
            .entry(attribute)
            .or_insert_with(|| IndexSearchAttributeStats {
                frequencies: BTreeMap::new(),
                sizes: BTreeMap::new(),
            });
        let (size, _matches) = stat.sizes.entry(entry).or_default();
        *size += count;
    }

    /// Check if any stats have been collected
    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
    }
}

/// Search statistics from the index
#[derive(Debug)]
pub struct IndexSearchAttributeStats {
    /// Average value occurrences within the searched collection
    pub frequencies: BTreeMap<Value, usize>,
    /// Matched entry attribute sizes
    /// bool indicates if the entry was matched
    pub sizes: BTreeMap<EntryIndex, (usize, bool)>,
}

#[cfg(test)]
#[allow(dead_code)]
fn make_sure_it_can_be_a_trait_object(_: Box<dyn IndexSearch>) {
    // ths fn just checks at compile time that the IndexSearch can be a trait object
}
