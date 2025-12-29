use std::collections::VecDeque;

use serde::Deserialize;
use tracing::trace;

use super::*;
use crate::index::prelude::Filter;
use crate::index::trivial::Trivial;
use crate::query::results::Score;
use crate::transaction::{Read, TransactionState};

mod boolean;
mod integer;
mod tag;

#[derive(Default)]
enum Finder<V: IndexableValue, F> {
    Loading {
        attr: AttributeIndex,
        filter: F,
        state: TransactionState<Read<Index<V>>, Index<V>>,
        stats: IndexSearchStats,
    },
    Iterating {
        results: VecDeque<IndexSearchEvent>,
    },
    #[default]
    Done,
}

impl<V, F> Finder<V, F>
where
    V: IndexableValue,
    V: for<'de> Deserialize<'de>,
{
    fn new(revision: u64, index: &Trivial<V>, attr: AttributeIndex, filter: F) -> Self {
        Self::Loading {
            attr,
            filter,
            state: TransactionState::read(
                revision,
                Trivial::<V>::name().into(),
                index.writer.load_full(),
                index.reader.clone(),
            ),
            stats: IndexSearchStats::default(),
        }
    }
}

impl<V, F> Iterator for Finder<V, F>
where
    V: for<'de> Deserialize<'de>,
    V: IndexableValue,
    F: Filter<V>,
{
    type Item = IndexSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            break match std::mem::take(self) {
                Finder::Done => None,
                Finder::Iterating { mut results } => {
                    let next = results.pop_front();
                    *self = Finder::Iterating { results };
                    next
                }
                Finder::Loading {
                    attr,
                    filter,
                    mut state,
                    mut stats,
                } => {
                    match state.load()? {
                        Ok(index) => {
                            trace!(msg = "filtering", ?filter, ?index);
                            // we have loaded and will just iterate now
                            *self = Self::Iterating {
                                results: filter
                                    .get(index)
                                    .filter_map(|(value, attrs)| {
                                        attrs.get(&attr).map(|entries| (value, entries))
                                    })
                                    .flat_map(|(value, entries)| {
                                        entries
                                            .iter()
                                            .map(move |(entry, indices)| (*entry, value, indices))
                                    })
                                    .fold(BTreeMap::new(), |mut map: BTreeMap<EntryIndex,  Vec<MatchedIndexTerm>>, (entry, value, indices)| {
                                        let matched = MatchedIndexTerm {
                                            value: value.clone().into_value(),
                                            score: Score::EXACT,
                                            positions: indices
                                                .iter()
                                                .copied()
                                                .map(|index| (attr, index, TokenPosition(0)))
                                                .collect(),
                                        };
                                        stats.matched(entry, &matched,
                                            |attribute|{
                                                get_frequencies(index, value, attribute)
                                            },
                                            |attribute,entry|{
                                                get_size(index,attribute,entry)
                                            },
                                            |attribute|{
                                                get_stats(index,attribute)
                                            }
                                        );

                                        map.entry(entry).or_default().push(matched);
                                        map
                                    })
                                    .into_iter()
                                    .map(|(entry, matched)| IndexSearchEvent::Found(entry, matched))
                                    .chain(std::iter::once(IndexSearchEvent::Stats(stats)))
                                    .collect(),
                            };
                            continue;
                        }
                        Err(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading {
                                attr,
                                filter,
                                state,
                                stats,
                            };
                            Some(IndexSearchEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}

/// Get value frequency in given attr
fn get_frequencies<V: Ord>(index: &Index<V>, value: &V, attribute: AttributeIndex) -> usize {
    index
        .get(value)
        .and_then(|attrs| attrs.get(&attribute))
        .map(|entries| entries.values())
        .into_iter()
        .flatten()
        .map(|placements| placements.len())
        .sum()
}

/// Get entry attr size in terms of tokens
fn get_size<V>(index: &Index<V>, attribute: AttributeIndex, entry: EntryIndex) -> usize {
    index
        .values()
        .flat_map(|attrs| attrs.get(&attribute))
        .flat_map(|entries: &BTreeMap<EntryIndex, BTreeSet<ValueIndex>>| entries.get(&entry))
        .map(|placements| placements.len())
        .sum()
}

/// Get attributes stats - entry count and average size in terms of tokens
fn get_stats<V>(index: &Index<V>, attribute: AttributeIndex) -> (usize, f64) {
    let mut entries = BTreeSet::new();
    let sizes = index
        .values()
        .flat_map(|attrs| attrs.get(&attribute))
        .flat_map(|entries: &BTreeMap<EntryIndex, BTreeSet<ValueIndex>>| entries.iter())
        .map(|(e, placements)| {
            entries.insert(e);
            placements.len()
        })
        .sum::<usize>();
    (entries.len(), sizes as f64 / entries.len() as f64)
}
