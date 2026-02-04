//! Text indexing module for efficient textual value storage and filtering.

#![warn(missing_docs)]

mod export;
pub mod filter;
mod search;
mod stats;
mod store;
mod value;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;

use indexmap::IndexSet;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};
// Re-export value types for public access
pub use value::*;

use self::stats::Stats;
use crate::index::prelude::*;
use crate::index::text::term::{TextTerm, TextTermTrigram};
use crate::index::text::trigram::{TrigramPosition, Trigrams};
use crate::query::expression::TermValuePart;
use crate::query::results::Score;

/// Map of value indices to sets of token positions.
pub type TokenOccurrences = BTreeMap<ValueIndex, BTreeSet<TokenPosition>>;

/// A token entry: the token string and its occurrences.
pub type TokenEntry = (Box<str>, BTreeMap<OccurrenceRef, TokenOccurrences>);

/// Trigram mapping for fuzzy search.
pub type TrigramMapping = BTreeMap<Trigram, BTreeMap<TrigramPosition, BTreeSet<TokenRef>>>;

//pub type Trigram = Box<str>;
pub type Trigram = crate::index::text::trigram::Trigram;

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
    /// get matching token ids (not TokenIndex) optionally matching prefix
    #[instrument(skip(self))]
    fn get_token_ids_exact(&self, term: &TextTerm) -> BTreeSet<TokenRef> {
        let mut matches = BTreeSet::default();

        let TextTerm {
            is_wild: is_wild_term,
            text,
            trigrams,
        } = term;
        trace!(
            msg = "get_token_ids_exact called",
            trigrams = trigrams.len(),
            is_wild_term,
            text_len = text.len()
        );

        #[allow(clippy::expect_used)]
        let value_len = u8::try_from(text.len()).expect("too long a token / search term");

        for TextTermTrigram {
            is_wild,
            position: pos,
            trigram,
        } in trigrams.iter().copied()
        {
            tracing::trace!(
                "Processing trigram at position {} with handle: {:?}",
                pos.offset(),
                trigram
            );

            let range = if is_wild {
                // wild trigram is a trigram after a wildcard
                // so search beyond the position
                pos..=TrigramPosition::new(u8::MAX)
            } else {
                // exact position
                pos..=pos
            };

            let tokens: BTreeSet<TokenRef> = self
                .trigrams
                .get(&trigram)
                .into_iter()
                .flat_map(|placements| placements.range(range.clone()))
                .flat_map(|(_, v)| v)
                .copied()
                .collect();

            if tokens.is_empty() {
                tracing::trace!(
                    "No tokens found for trigram at position {}, clearing matches",
                    pos.offset()
                );
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
                    let keep = *is_wild_term && (len >= value_len) || len == value_len;
                    tracing::trace!(
                        "Token {} (len: {}) for value '{:?}' (len: {}): keep = {}",
                        self.token(*token_id),
                        len,
                        term.text,
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
            "get_token_ids_exact result: {} matching tokens for {:?}",
            matches.len(),
            term.text
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

impl Hash for TextIndex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            occurrences,
            tokens,
            trigrams,
            stats,
        } = self;
        occurrences.iter().enumerate().for_each(|o| o.hash(state));
        tokens.hash(state);
        trigrams.hash(state);
        stats.hash(state);
    }
}

#[cfg(test)]
mod tests;
