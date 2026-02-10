//! Index searches.

use std::collections::BTreeMap;

pub use crate::document::Value;
use crate::index::prelude::*;
use crate::query::expression::Func;
use crate::query::option::QueryOptions;
use crate::query::results::Score;
use crate::transaction::LoadEvent;

/// Index filter
pub trait IndexSearch {
    /// Search the index at revision for a value by function and optional attribute
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
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

impl IndexSearchStats {
    /// Update stats with a match.
    /// The stats may ask for additional information - frequencies, sizes, totals
    /// frequencies - value frequency in given attr
    /// sizes - matched entry attribute token count
    /// total - collection (entries count, average size)
    pub fn matched(
        &mut self,
        entry: EntryIndex,
        matched: &MatchedIndexTerm,
        frequencies: impl Fn(AttributeIndex) -> usize,
        sizes: impl Fn(AttributeIndex, EntryIndex) -> usize,
        totals: impl Fn(AttributeIndex) -> (usize, f64),
    ) {
        for (attribute, ..) in &matched.positions {
            // fetch missing attribute frequencies and entry sizes
            let stat = self.stats.entry(*attribute).or_insert_with(|| {
                let (entries, size) = totals(*attribute);
                IndexSearchAttributeStats {
                    entries,
                    size,
                    frequencies: BTreeMap::new(),
                    sizes: BTreeMap::new(),
                }
            });

            stat.frequencies
                .entry(matched.value.clone())
                .or_insert_with(|| frequencies(*attribute));

            stat.sizes
                .entry(entry)
                .or_insert_with(|| sizes(*attribute, entry));
        }
    }
}

/// Search statistics from the index
#[derive(Debug)]
pub struct IndexSearchAttributeStats {
    /// How many entries exist in the searched collection in total (regardles of search)
    pub entries: usize,
    /// The average entry size
    pub size: f64,
    /// Average value occurrences within the searched collection
    pub frequencies: BTreeMap<Value, usize>,
    /// Matched entry attribute sizes
    /// This does not include entries that were not matched
    pub sizes: BTreeMap<EntryIndex, usize>,
}

#[cfg(test)]
#[allow(dead_code)]
fn make_sure_it_can_be_a_trait_object(_: Box<dyn IndexSearch>) {
    // ths fn just checks at compile time that the IndexSearch can be a trait object
}
