use std::collections::VecDeque;
use std::ops::ControlFlow;

use serde::Deserialize;
use tracing::trace;

use super::*;
use crate::blob::{Blob, BlobId};
use crate::index::prelude::Filter;
use crate::index::trivial::Trivial;
use crate::query::results::Score;

mod boolean;
mod integer;
mod tag;

#[derive(Default)]
enum Finder<V: IndexableValue, F> {
    Loading {
        attr: AttributeIndex,
        filter: F,
        blob: Blob<Index<V>>,
        stats: Option<IndexSearchStats>,
        collect_positions: bool,
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
    fn new(
        _index: &Trivial<V>,
        revision: BlobId,
        attr: AttributeIndex,
        filter: F,
        collect_stats: bool,
        collect_positions: bool,
    ) -> Self {
        Self::Loading {
            attr,
            filter,
            blob: Blob::new(revision, Trivial::<V>::name()),
            stats: collect_stats.then_some(IndexSearchStats::default()),
            collect_positions: collect_positions || collect_stats,
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
                    blob,
                    mut stats,
                    collect_positions,
                } => {
                    match blob.load() {
                        ControlFlow::Continue(index) => {
                            trace!(msg = "filtering", ?filter, ?index);
                            if let Some(stats) = &mut stats {
                                update_stats(index, stats);
                            }
                            // we have loaded and will just iterate now
                            *self = Self::Iterating {
                                results: filter
                                    .filter(index)
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
                                            value: value.clone().into(),
                                            score: Score::EXACT,
                                            positions: indices
                                                .iter()
                                                .copied()
                                                .map(|index| (attr, index, TokenPosition(0)))
                                                .collect(),
                                        };

                                        if let Some(stats) = &mut stats {
                                            stats.matched(entry, &matched);
                                        }

                                        let positions = map.entry(entry).or_default();
                                        if collect_positions{
                                            positions.push(matched);
                                        }
                                        map
                                    })
                                    .into_iter()
                                    .map(|(entry, matched)| IndexSearchEvent::Found(entry, matched))
                                    .chain(stats.map(IndexSearchEvent::Stats))
                                    .collect(),
                            };
                            continue;
                        }
                        ControlFlow::Break(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading {
                                attr,
                                filter,
                                blob,
                                stats,
                                collect_positions,
                            };
                            Some(IndexSearchEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}

fn update_stats<V>(index: &Index<V>, stats: &mut IndexSearchStats) {
    for occurrences in index.values() {
        for (a, entries) in occurrences {
            for (e, placements) in entries {
                stats.increment_size(*a, *e, placements.len());
            }
        }
    }
}
