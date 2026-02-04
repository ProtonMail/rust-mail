use std::collections::HashSet;

use tracing::trace;

use super::filter::*;
use super::*;
use crate::chunker::ChunkIter;
use crate::document::Value;
use crate::index::text::distance;
use crate::index::text::term::TextTerm;
use crate::query::results::Score;

impl TextIndex {
    /// search through the exact terms and returns the number of occurrences in the document as a score
    #[tracing::instrument(skip_all)]
    pub(crate) fn search_equals(
        &self,
        filter: &EqualsTextFilter,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&HashSet<EntryIndex>>,
        collect_stats: bool,
        collect_positions: bool,
    ) -> Option<(IndexSearchResults, Option<IndexSearchStats>)> {
        let term = TextTerm::new(filter.term.wildcard_text()?);
        if term.is_wild && term.text.is_empty() {
            // existential check shortcut: entry matches if it has any text
            return Some(self.find_entries_with_any_text(attr_filter));
        }
        let mut stats = collect_stats.then_some(IndexSearchStats::default());
        let result = self.search_exact(&term, attr_filter, universe, &mut stats);
        Some(self.compute_exact_result(&term, result, stats, collect_positions || collect_stats))
    }

    /// Reusable searching returning preliminary results
    /// Returning option just for some syntax shortcuts.
    #[tracing::instrument(skip(self), fields(term, attr_filter, universe))]
    fn search_exact(
        &self,
        term: &TextTerm,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&HashSet<EntryIndex>>,
        stats: &mut Option<IndexSearchStats>,
    ) -> BTreeSet<(
        TokenRef,
        EntryIndex,
        AttributeIndex,
        ValueIndex,
        TokenPosition,
    )> {
        let token_ids: BTreeSet<TokenRef> = self.get_token_ids_exact(term);
        trace!(msg = "Found token IDs", count = token_ids.len(), ?term);

        let result = token_ids
            .into_iter()
            .flat_map(|token_id| {
                self.get_occurrences(token_id, universe, attr_filter)
                    .map(move |(e, a, v, t)| (token_id, e, a, v, t))
            })
            .collect::<BTreeSet<_>>();

        tracing::trace!("search_exact result: {} occurrences", result.len());
        if !result.is_empty()
            && let Some(stats) = stats
        {
            self.stats.update_stats(stats);
        }
        result
    }

    /// The order of keys in the sorted btree set matters as it is used for chunking consecutive blocks
    fn compute_exact_result(
        &self,
        term: &TextTerm,
        result: BTreeSet<(
            TokenRef,
            EntryIndex,
            AttributeIndex,
            ValueIndex,
            TokenPosition,
        )>,
        mut stats: Option<IndexSearchStats>,
        collect_positions: bool,
    ) -> (IndexSearchResults, Option<IndexSearchStats>) {
        let entries = result
            .into_iter()
            .chunk(|(r, e, _a, _v, _t)| (*r, *e), |(_r, _e, a, v, t)| (a, v, t));

        let result = entries.fold(
            BTreeMap::new(),
            |mut map: IndexSearchResults, ((token_id, entry), items)| {
                let token = self.token(token_id);
                let score = if token == term.text.as_ref() {
                    Score::EXACT
                } else {
                    let distance = distance::levenshtein(token, &term.text);
                    let score = distance::ratio(token.len(), term.text.len(), distance as f64);
                    Score::new(score)
                };

                let matched = MatchedIndexTerm {
                    value: Value::text(token),
                    score,
                    positions: items.collect(),
                };

                if let Some(stats) = &mut stats {
                    stats.matched(entry, &matched);
                }

                let positions = map.entry(entry).or_default();
                if collect_positions {
                    positions.push(matched);
                }
                map
            },
        );

        (result, stats)
    }
}

#[cfg(test)]
#[path = "tests_exact.rs"]
mod tests_exact;
