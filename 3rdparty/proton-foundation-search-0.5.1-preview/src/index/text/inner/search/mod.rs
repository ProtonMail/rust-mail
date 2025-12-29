use std::collections::{BTreeMap, HashSet};

use super::*;
use crate::index::text::inner::filter::TextFilter;

mod exact;
mod fuzzy;

/// Abstraction making switch on feature easier
/// Now supports multiple attributes per entry, with the best score per attribute.
type IndexSearchResults = BTreeMap<EntryIndex, Vec<MatchedIndexTerm>>;

impl TextIndex {
    /// Performs a text search based on the given filter criteria
    ///
    /// This method delegates to specific search implementations based on the filter type:
    /// - `TextFilter::Equals` -> exact match search
    /// - `TextFilter::StartsWith` -> prefix search
    /// - `TextFilter::Matches` -> fuzzy/approximate search
    ///
    /// # Arguments
    ///
    /// * `filter` - The search filter specifying what to search for
    /// * `attr_filter` - Optional attribute filter to limit search scope
    /// * `universe_filter` - Optional set of entry indices to limit search scope
    ///
    /// # Returns
    ///
    /// A `SearchScoreMap` containing the search results with relevance scores
    #[tracing::instrument(skip(self, universe_filter))]
    pub fn search(
        &self,
        filter: &TextFilter,
        attr_filter: Option<AttributeIndex>,
        universe_filter: Option<&HashSet<EntryIndex>>,
    ) -> (IndexSearchResults, IndexSearchStats) {
        tracing::trace!(
            "TextIndex::search called with filter: {:?}, attr_filter: {:?}, universe_filter: {:?}",
            filter,
            attr_filter,
            universe_filter
        );

        // Debug assertion: Dump index contents
        #[cfg(debug_assertions)]
        {
            tracing::debug!("=== INDEX CONTENTS DUMP ===");
            tracing::debug!("Tokens ({}):", self.tokens.len());
            for (i, (token, _occurrences)) in self.tokens.iter().enumerate() {
                tracing::debug!(
                    "  [{}] Token: '{}' (len: {})",
                    i,
                    token.as_ref(),
                    token.len()
                );
            }

            tracing::debug!("Trigrams ({}):", self.trigrams.len());
            for (trigram, placements) in &self.trigrams {
                tracing::debug!("  Trigram '{}':", trigram);
                for (pos, tokens) in placements {
                    tracing::debug!("    Position {}: {} tokens", pos.offset(), tokens.len());
                    for token_ref in tokens.iter().take(5) {
                        // Show first 5 tokens
                        let token_str = self.token(*token_ref);
                        tracing::debug!("      TokenRef({:?}) -> '{}'", token_ref, token_str);
                    }
                    if tokens.len() > 5 {
                        tracing::debug!("      ... and {} more", tokens.len() - 5);
                    }
                }
            }
            tracing::debug!("Occurrences ({}):", self.occurrences.len());
            for (i, (entry, attr)) in self.occurrences.iter().enumerate() {
                tracing::debug!("  [{}] Entry({}), Attribute({})", i, entry.0, attr.0);
            }
            tracing::debug!("=== END INDEX CONTENTS DUMP ===");
        }

        let result = match filter {
            TextFilter::Equals(filter) => self.search_equals(filter, attr_filter, universe_filter),
            TextFilter::StartsWith(filter) => {
                self.search_starts_with(filter, attr_filter, universe_filter)
            }
            TextFilter::Matches(filter) => {
                self.search_matches(filter, attr_filter, universe_filter)
            }
        };

        tracing::trace!(
            "TextIndex::search result: {} entries",
            result.as_ref().map(|(r, _s)| r.len()).unwrap_or(0)
        );

        result.unwrap_or_default()
    }
}
