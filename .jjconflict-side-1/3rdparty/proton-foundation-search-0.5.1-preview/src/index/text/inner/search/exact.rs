use std::collections::HashSet;

use super::*;
use crate::chunker::ChunkIter;
use crate::index::text::inner::filter::*;
use crate::query::results::Score;

impl TextIndex {
    /// search through the exact terms and returns the number of occurrences in the document as a score
    #[tracing::instrument(skip_all)]
    pub(crate) fn search_equals(
        &self,
        filter: &EqualsTextFilter,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&HashSet<EntryIndex>>,
    ) -> Option<(IndexSearchResults, IndexSearchStats)> {
        let EqualsTextFilter { term } = filter;
        let result = self.search_exact(term, false, attr_filter, universe)?;
        Some(self.compute_exact_result(result))
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn search_starts_with(
        &self,
        filter: &StartsWithTextFilter,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&HashSet<EntryIndex>>,
    ) -> Option<(IndexSearchResults, IndexSearchStats)> {
        let StartsWithTextFilter { prefix } = filter;
        let result = self.search_exact(prefix, true, attr_filter, universe)?;
        Some(self.compute_exact_result(result))
    }

    /// Reusable searching by prefix returning preliminary results
    /// so it can be used in both prefix and exact search.
    /// Returning option just for some syntax shortcuts.
    #[tracing::instrument(skip(self), fields(term, prefix, attr_filter, universe))]
    fn search_exact(
        &self,
        term: &str,
        prefix: bool,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&HashSet<EntryIndex>>,
    ) -> Option<
        BTreeSet<(
            TokenRef,
            EntryIndex,
            AttributeIndex,
            ValueIndex,
            TokenPosition,
        )>,
    > {
        let token_ids: BTreeSet<TokenRef> = self.get_token_ids_exact(term, prefix);
        tracing::trace!("Found {} token IDs for term '{}'", token_ids.len(), term);

        let result = token_ids
            .into_iter()
            .flat_map(|token_id| {
                self.get_occurrences(token_id, universe, attr_filter)
                    .map(move |(e, a, v, t)| (token_id, e, a, v, t))
            })
            .collect::<BTreeSet<_>>();

        tracing::trace!("search_exact result: {} occurrences", result.len());
        Some(result)
    }

    /// The order of keys in the sorted btree set matters as it is used for chunking consecutive blocks
    fn compute_exact_result(
        &self,
        result: BTreeSet<(
            TokenRef,
            EntryIndex,
            AttributeIndex,
            ValueIndex,
            TokenPosition,
        )>,
    ) -> (IndexSearchResults, IndexSearchStats) {
        let entries = result
            .into_iter()
            .chunk(|(r, e, _a, _v, _t)| (*r, *e), |(_r, _e, a, v, t)| (a, v, t));
        let mut stats = IndexSearchStats::default();

        let result = entries.fold(
            BTreeMap::new(),
            |mut map: IndexSearchResults, ((token_id, entry), items)| {
                let matched = MatchedIndexTerm {
                    value: Value::text(self.token(token_id)),
                    score: Score::EXACT,
                    positions: items.collect(),
                };

                stats.matched(
                    entry,
                    &matched,
                    |attribute| {
                        self.get_occurrences(token_id, None, Some(attribute))
                            .count()
                    },
                    |attribute, entry| {
                        let (_length, count) = self.stats.get(entry, attribute).unwrap_or_default();
                        count
                    },
                    |attribute| {
                        let entries = self.stats.entries(attribute);
                        let count = self.stats.count(attribute);
                        (entries, count as f64 / entries as f64)
                    },
                );

                map.entry(entry).or_default().push(matched);
                map
            },
        );

        (result, stats)
    }
}

#[cfg(test)]
#[path = "tests_exact.rs"]
mod tests_exact;
