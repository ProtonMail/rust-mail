//! Text indexing module for efficient textual value storage and filtering.

#![warn(missing_docs)]

pub mod additive;
#[cfg(debug_assertions)]
mod debug;
mod distance;
mod export;
pub mod filter;
mod search;
mod stats;
mod store;
mod trigram;
mod value;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub use additive::AdditiveTextIndex;
use indexmap::IndexSet;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::instrument;
// Re-export value types for public access
pub use value::*;

use crate::index::prelude::*;
use crate::index::text::inner::stats::Stats;
use crate::index::text::trigram::Trigrams;
use crate::query::results::Score;

/// Map of value indices to sets of token positions.
pub type TokenOccurrences = BTreeMap<ValueIndex, BTreeSet<TokenPosition>>;

/// A token entry: the token string and its occurrences.
pub type TokenEntry = (Box<str>, BTreeMap<OccurrenceRef, TokenOccurrences>);

/// Trigram mapping for fuzzy search.
pub type TrigramMapping = BTreeMap<Trigram, BTreeMap<TrigramPosition, BTreeSet<TokenRef>>>;

//pub type Trigram = Box<str>;
pub type Trigram = trigram::Trigram;

/// Text index structure and search implementation based on trigrams
///
/// Trigrams are stored efficiently in the token cloud and handles to these are used in the inverse index.
///
/// The cloud does not track token length so each trigram, when retrieved from the cloud must be truncated to three characters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextIndex {
    /// List of entry-attr-value being indexed.
    /// The order is essential as the offset ref is used in tokens.
    occurrences: IndexSet<(EntryIndex, AttributeIndex)>,
    /// List of unique tokens (words) and their occurrences.
    /// The occurrence reference is an offset in the occurrences list
    /// The order is essential as the offset ref is used in trigrams.
    tokens: Vec<TokenEntry>,
    /// Mapping of trigrams by position to tokens
    /// The token reference is an offset in the tokens list
    trigrams: TrigramMapping,
    /// Index statistics
    stats: Stats,
}

impl TextIndex {
    /// Internal method to access occurrences for efficient conversion
    pub(crate) fn occurrences_mut(&mut self) -> &mut IndexSet<(EntryIndex, AttributeIndex)> {
        &mut self.occurrences
    }

    /// Internal method to access tokens for efficient conversion
    pub(crate) fn tokens_mut(&mut self) -> &mut Vec<TokenEntry> {
        &mut self.tokens
    }

    /// Internal method to access trigrams for efficient conversion
    pub(crate) fn trigrams_mut(&mut self) -> &mut TrigramMapping {
        &mut self.trigrams
    }

    /// Public method to access stats for read-only operations
    ///
    /// Returns a reference to the index statistics
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Debug helper function to log trigram information at a specific position
    fn debug_log_trigram(value: &str, pos: usize) {
        if pos + 3 <= value.len() && value.is_char_boundary(pos) && value.is_char_boundary(pos + 3)
        {
            let trigram = &value[pos..pos + 3];
            tracing::debug!("  Position {}: trigram '{}'", pos, trigram);
        } else {
            tracing::debug!(
                "  Position {}: invalid UTF-8 boundary for trigram slice",
                pos
            );
        }
    }

    /// get matching token ids (not TokenIndex) optionally matching prefix
    fn get_token_ids_exact(&self, value: impl AsRef<str>, prefix: bool) -> BTreeSet<TokenRef> {
        let mut matches = BTreeSet::default();
        let value = value.as_ref();
        tracing::trace!(
            "get_token_ids_exact called with value: '{}', prefix: {}, trigram count: {}",
            value,
            prefix,
            value.trigrams().count()
        );

        // Debug assertion: Show what trigrams we're looking for
        #[cfg(debug_assertions)]
        {
            tracing::debug!("Looking for trigrams in '{}':", value);
            for (pos, _handle) in value.trigrams() {
                Self::debug_log_trigram(value, pos);
            }
        }

        #[allow(clippy::expect_used)]
        let value_len = u8::try_from(value.len()).expect("too long a token / search term");

        for (pos, handle) in value.trigrams() {
            let pos = TrigramPosition::new(pos as u8);
            tracing::trace!(
                "Processing trigram at position {} with handle: {:?}",
                pos.offset(),
                handle
            );

            let Some(tokens) = self
                .trigrams
                .get(handle)
                .and_then(|placements| placements.get(&pos))
            else {
                tracing::trace!(
                    "No tokens found for trigram at position {}, clearing matches",
                    pos.offset()
                );
                // Debug assertion: Validate trigram boundaries and log details
                #[cfg(debug_assertions)]
                {
                    Self::debug_log_trigram(value, pos.offset());
                }
                matches.clear();
                break;
            };

            tracing::trace!(
                "Found {} tokens for trigram at position {}",
                tokens.len(),
                pos.offset()
            );

            if pos.offset() == 0 {
                // The first trigram search will initialize the result set.
                matches = tokens.clone();
                matches.retain(|token_id| {
                    let len = self.token_len(*token_id);
                    let keep = prefix && (len >= value_len) || len == value_len;
                    tracing::trace!(
                        "Token {} (len: {}) for value '{}' (len: {}): keep = {}",
                        self.token(*token_id),
                        len,
                        value,
                        value_len,
                        keep
                    );
                    keep
                });
                tracing::trace!(
                    "After first trigram filter: {} tokens remaining",
                    matches.len()
                );
            } else {
                // We have already initialized the result with a search for the first trigram.
                // Continue reducing the result set by matching further trigrams
                let before_count = matches.len();
                matches.retain(|t| tokens.contains(t));
                tracing::trace!(
                    "After trigram {} filter: {} -> {} tokens",
                    pos.offset(),
                    before_count,
                    matches.len()
                );
            }
            if matches.is_empty() {
                tracing::trace!("No matches remaining, breaking early");
                break;
            }
        }

        tracing::trace!(
            "get_token_ids_exact result: {} matching tokens for '{}'",
            matches.len(),
            value
        );
        matches
    }

    /// Get an indexed token. Panics if it does not exist.
    fn token(&self, reference: TokenRef) -> &str {
        let (token, _occurrences) = &self.tokens[reference.offset()];
        token.as_ref()
    }
    /// Get an indexed token length. Panics if it does not exist.
    fn token_len(&self, reference: TokenRef) -> u8 {
        let (token, _occurrences) = &self.tokens[reference.offset()];
        token.len() as u8
    }
    /// Get occurrences of an indexed token. Panics if it does not exist.
    fn token_occurrences(
        &self,
        reference: TokenRef,
    ) -> &BTreeMap<OccurrenceRef, BTreeMap<ValueIndex, BTreeSet<TokenPosition>>> {
        let (_length, occurrences) = &self.tokens[reference.offset()];
        occurrences
    }
    /// Get an indexed token. Panics if it does not exist.
    fn token_occurrences_mut(
        &mut self,
        reference: TokenRef,
    ) -> &mut BTreeMap<OccurrenceRef, BTreeMap<ValueIndex, BTreeSet<TokenPosition>>> {
        let (_length, occurrences) = &mut self.tokens[reference.offset()];
        occurrences
    }
    /// Get an indexed occurrence. Panics if it does not exist.
    fn occurrence(&self, reference: OccurrenceRef) -> (EntryIndex, AttributeIndex) {
        self.occurrences[reference.offset()]
    }
    fn get_occurrences(
        &self,
        token_id: TokenRef,
        entry_filter: Option<&HashSet<EntryIndex>>,
        attr_filter: Option<AttributeIndex>,
    ) -> impl Iterator<Item = (EntryIndex, AttributeIndex, ValueIndex, TokenPosition)> {
        let token_str = self.token(token_id);
        tracing::trace!(
            "get_occurrences called for token '{}' (id: {:?}), entry_filter: {:?}, attr_filter: {:?}",
            token_str,
            token_id,
            entry_filter,
            attr_filter
        );

        let occurrences = self.token_occurrences(token_id);
        tracing::trace!(
            "Found {} occurrence references for token '{}'",
            occurrences.len(),
            token_str
        );

        let result: Vec<_> = occurrences
            .iter()
            .flat_map(|(occurrence, tokens)| tokens.iter().map(move |t| (occurrence, t)))
            .filter_map(move |(occurrence, (v, tokens))| {
                let (e, a) = self.occurrence(*occurrence);
                let matched = entry_filter
                    .map(|entry_filter| entry_filter.contains(&e))
                    .unwrap_or(true)
                    && attr_filter
                        .map(|attr_filter| a == attr_filter)
                        .unwrap_or(true);

                if matched {
                    tracing::trace!(
                        "Occurrence matched: Entry({}), Attribute({}), Value({}), {} token positions",
                        e.0, a.0, v.0, tokens.len()
                    );
                }

                matched.then_some((e, a, *v, tokens))
            })
            .flat_map(|(e, a, v, t)| t.iter().map(move |t| (e, a, v, *t)))
            .collect();

        tracing::trace!(
            "get_occurrences result: {} occurrences for token '{}'",
            result.len(),
            token_str
        );

        result.into_iter()
    }
}

#[cfg(test)]
mod tests;
