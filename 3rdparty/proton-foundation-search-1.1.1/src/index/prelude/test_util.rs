//! index testing utilities
use std::collections::BTreeMap;

use super::*;
use crate::blob::{BlobId, Cached, LoadEvent, ReleaseEvent, SaveEvent};
use crate::query::expression::{Func, Operator, TermValue};
use crate::query::option::QueryOptions;
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};
use crate::query::results::{
    FoundEntry, MatchGroup, MatchNode, MatchOccurrence, MatchValue, Score,
};
use crate::query::stats::{AttributeStats, CollectionStats};

/// Insert fixture
pub fn add_index_contents(
    index: &dyn Index,
    revision: &mut Option<BlobId>,
    cache: &mut BTreeMap<BlobId, Cached>,
) {
    let write = index.write(
        *revision,
        &[
            IndexStoreOperation::Insert(
                EntryIndex(0),
                AttributeIndex(0),
                vec![EntryValue::text(["hello", "world"])].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(0),
                AttributeIndex(1),
                vec![EntryValue::text(["hello", "world"])].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(0),
                vec![EntryValue::text(["the", "world", "say", "hello"])].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(2),
                AttributeIndex(0),
                vec![EntryValue::text(["hello"])].into(),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(3),
                AttributeIndex(0),
                vec![EntryValue::text(["just", "wanted", "say"])].into(),
            ),
        ],
    );
    *revision = handle_write(write, cache);
}

/// Get a simplified search result
pub fn test_search_matches(
    sut: &dyn Index,
    term: &str,
    rev: BlobId,
    cache: &BTreeMap<BlobId, Cached>,
    max_distance: usize,
    min_similarity: f64,
) -> Vec<(Score, EntryIndex)> {
    let search = sut
        .search(
            rev,
            None,
            Func::Matches,
            &TermValue::text(term),
            &QueryOptions::default()
                .with(|o: &mut MaximumDistance| *o = max_distance.into())
                .with(|o: &mut MinimumSimilarity| *o = min_similarity.into()),
        )
        .expect("search");

    let mut stats = IndexSearchStats::default();
    let mut result = test_util::handle_search(search, cache, &mut stats)
        .map(|(e, m)| {
            FoundEntry::new_with_matches(
                format!("entry{}", e.0),
                MatchGroup::new(
                    Operator::Or,
                    m.into_iter().map(|m| {
                        MatchNode::Value(MatchValue::new(
                            m.value,
                            m.score,
                            m.positions
                                .into_iter()
                                .map(|(a, v, t)| MatchOccurrence::new(format!("attr{}", a.0), v, t))
                                .collect(),
                        ))
                    }),
                ),
            )
        })
        .collect::<Vec<_>>();

    let stats = CollectionStats::new(stats.into_iter().map(|(a, attr_stats)| {
        let count = attr_stats.sizes.len();
        let mut total = 0;
        let sizes = attr_stats
            .sizes
            .into_iter()
            .filter_map(|(e, (size, matched))| {
                total += size;
                if matched {
                    Some((format!("entry{}", e.0).into_boxed_str(), size))
                } else {
                    None
                }
            })
            .collect();
        (
            format!("attr{}", a.0).into_boxed_str(),
            AttributeStats::new(
                count,
                total as f64 / count as f64,
                attr_stats.frequencies,
                sizes,
            ),
        )
    }));

    stats.update_all_scores(&mut result);

    result.sort();

    result
        .into_iter()
        .map(|found| {
            (
                Score::new((found.score().value() * 1000.0).round() / 1000.0),
                EntryIndex(
                    found.identifier()[5..]
                        .parse::<u32>()
                        .unwrap_or_else(|_| panic!("entry index for {}", found.identifier())),
                ),
            )
        })
        .collect()
}

/// Handle index write with a cache
pub fn handle_write(
    write: impl Iterator<Item = IndexStoreEvent>,
    cache: &mut BTreeMap<BlobId, Cached>,
) -> Option<BlobId> {
    let mut rev = None;
    for event in write {
        match event {
            IndexStoreEvent::Inserted { .. } | IndexStoreEvent::Removed { .. } => {}
            IndexStoreEvent::Load(load_event) => load(load_event, cache),
            IndexStoreEvent::Save(save_event) => save(save_event, cache),
            IndexStoreEvent::Release(release_event) => release(release_event, cache),
            IndexStoreEvent::Revision(revision_event) => rev = Some(revision_event.id()),
        }
    }
    rev
}

/// Handle index search with a cache
pub fn handle_search(
    search: impl Iterator<Item = IndexSearchEvent>,
    cache: &BTreeMap<BlobId, Cached>,
    stats: &mut IndexSearchStats,
) -> impl Iterator<Item = (EntryIndex, Vec<MatchedIndexTerm>)> {
    search.filter_map(|event| match event {
        IndexSearchEvent::Load(load_event) => {
            load(load_event, cache);
            None
        }
        IndexSearchEvent::Stats(s) => {
            *stats = s;
            None
        }
        IndexSearchEvent::Found(entry_index, matched_index_terms) => {
            Some((entry_index, matched_index_terms))
        }
    })
}

/// Release a blob from cache
pub fn release(release_event: ReleaseEvent, cache: &mut BTreeMap<BlobId, Cached>) {
    cache.remove(&release_event.id());
}

/// Save a blob
pub fn save(save_event: SaveEvent, cache: &mut BTreeMap<BlobId, Cached>) {
    cache.insert(save_event.id(), save_event.recv());
}

/// Load a blob
pub fn load(load_event: LoadEvent, cache: &BTreeMap<BlobId, Cached>) {
    match cache.get(&load_event.id()) {
        Some(cached) => load_event.send_cached(cached).expect("send cached"),
        None => {
            load_event.send_empty().expect("send empty");
        }
    }
}
