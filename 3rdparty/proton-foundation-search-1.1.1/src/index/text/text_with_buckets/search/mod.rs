#[cfg(test)]
mod tests_exact;
#[cfg(test)]
mod tests_fuzzy;
#[cfg(test)]
mod tests_fuzzy_large;

use alloc::collections::BTreeMap;
use core::ops::ControlFlow;

use tracing::{instrument, trace};

use super::content::Content;
use super::tokens::Tokens;
use super::vocabulary::TokenRef;
use crate::blob::{Blob, BlobId, LoadEvent};
use crate::document::Value;
use crate::index::prelude::*;
use crate::index::text::finding::Finding;
use crate::index::text::term::TextTerm;
use crate::prelude::*;

#[derive(Debug)]
pub struct Search {
    content: Blob<Content>,
    attribute: Option<AttributeIndex>,
    term: TextTerm,
    max_distance: usize,
    min_similarity: f64,
    take_the_wild_shortcut: bool,

    tokens_found: BTreeMap<TokenRef, Finding>,
    buckets_to_search: Vec<Blob<Tokens>>,
    entries_found: EntriesFound,
    stats: Option<IndexSearchStats>,
    collect_positions: bool,
}

type EntriesFound = BTreeMap<EntryIndex, BTreeMap<TokenRef, MatchedIndexTerm>>;

impl Iterator for Search {
    type Item = IndexSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        match self.progress() {
            ControlFlow::Break(event) => Some(event),
            ControlFlow::Continue(()) => None,
        }
    }
}

impl Search {
    pub fn new(
        revision: BlobId,
        attribute: Option<AttributeIndex>,
        term: TextTerm,
        max_distance: usize,
        min_similarity: f64,
        mut collect_stats: bool,
        mut collect_positions: bool,
    ) -> Self {
        let take_the_wild_shortcut = term.is_wild && term.text.is_empty();
        collect_stats &= !take_the_wild_shortcut;
        collect_positions |= collect_stats;
        Self {
            content: Blob::new(revision, "text index"),
            attribute,
            term,
            max_distance,
            min_similarity,
            tokens_found: Default::default(),
            buckets_to_search: Default::default(),
            entries_found: Default::default(),
            stats: collect_stats.then_some(Default::default()),
            collect_positions,
            take_the_wild_shortcut,
        }
    }

    #[inline(never)]
    fn progress(&mut self) -> ControlFlow<IndexSearchEvent> {
        self.find_tokens().map_break(IndexSearchEvent::Load)?;
        self.find_occurrences().map_break(IndexSearchEvent::Load)?;
        self.get_result()?;
        ControlFlow::Continue(())
    }

    #[inline(never)]
    #[instrument(skip(self))]
    fn find_tokens(&mut self) -> ControlFlow<LoadEvent> {
        if self.term.text.is_empty() || !self.tokens_found.is_empty() {
            return ControlFlow::Continue(());
        }

        let content = self.content.load()?;

        let trigrams = content
            .vocabulary
            .get_trigrams(self.term.trigram_lookup(self.max_distance));
        for (pos, _trigram, token_ref, token) in trigrams {
            let found = self
                .tokens_found
                .entry(token_ref)
                .or_insert_with(|| Finding::new(token.clone()));
            found.matches[pos.0 as usize / 64] |= 1 << (pos.0 % 64);
        }

        self.tokens_found.retain(|_token_ref, found| {
            found.satisfy(&self.term, self.max_distance, self.min_similarity)
        });

        trace!("{:#?}", self.tokens_found);
        if !self.tokens_found.is_empty() {
            self.buckets_to_search = content
                .occurrences
                .iter()
                .filter_map(|b| {
                    self.tokens_found
                        .keys()
                        .any(|tref| b.tokens.contains_key(tref))
                        .then_some(b.occurrences.clone())
                })
                .collect();
        }

        ControlFlow::Continue(())
    }

    #[inline(never)]
    #[instrument(skip(self))]
    fn find_occurrences(&mut self) -> ControlFlow<LoadEvent> {
        if self.take_the_wild_shortcut {
            // existence check, all entries qualify
            let content = self.content.load()?;
            let entries = content
                .occurrences
                .iter()
                .flat_map(|b| &b.entries)
                .filter(|(_, a, size)| {
                    *size != 0 && self.attribute.is_none() || self.attribute == Some(*a)
                })
                .map(|(e, _, _)| (*e, Default::default()));
            self.entries_found = entries.collect();
            trace!(?self.entries_found);
            self.take_the_wild_shortcut = false;
        }

        if self.buckets_to_search.is_empty() {
            return ControlFlow::Continue(());
        }

        loop {
            let Some(bucket) = self.buckets_to_search.last_mut() else {
                break ControlFlow::Continue(());
            };

            let occurrences = bucket.fetch_and_free()?;

            for (token_ref, found) in &self.tokens_found {
                let Some(occurrences) = occurrences.get(token_ref) else {
                    continue;
                };
                match &self.attribute {
                    Some(attr) => add_occurrences(
                        &mut self.entries_found,
                        found,
                        token_ref,
                        self.collect_positions,
                        occurrences.range(
                            (*attr, ValueIndex(0), EntryIndex(0), TokenPosition(0))
                                ..=(
                                    *attr,
                                    ValueIndex(usize::MAX),
                                    EntryIndex(u32::MAX),
                                    TokenPosition(usize::MAX),
                                ),
                        ),
                    ),
                    None => add_occurrences(
                        &mut self.entries_found,
                        found,
                        token_ref,
                        self.collect_positions,
                        occurrences,
                    ),
                }
            }

            // remove the last bucket once we load it
            self.buckets_to_search.pop();
        }
    }

    #[inline(never)]
    #[instrument(skip(self))]
    fn get_result(&mut self) -> ControlFlow<IndexSearchEvent> {
        let Some((entry, matches)) = self.entries_found.pop_first() else {
            let stats = self.finish_stats().map_break(IndexSearchEvent::Load)?;
            trace!(?stats);
            return match stats {
                None => ControlFlow::Continue(()),
                Some(stats) => ControlFlow::Break(IndexSearchEvent::Stats(stats)),
            };
        };

        if let Some(stats) = &mut self.stats {
            for matched in matches.values() {
                stats.matched(entry, matched);
            }
        }

        trace!(?entry, ?matches);

        ControlFlow::Break(IndexSearchEvent::Found(
            entry,
            matches.into_values().collect(),
        ))
    }

    #[inline(never)]
    #[instrument(skip(self))]
    fn finish_stats(&mut self) -> ControlFlow<LoadEvent, Option<IndexSearchStats>> {
        let Some(stats) = &mut self.stats else {
            return ControlFlow::Continue(None);
        };

        let content = self.content.load()?;

        for bucket in &content.occurrences {
            for (e, a, size) in &bucket.entries {
                stats.increment_size(*a, *e, *size);
            }
        }

        if stats.is_empty() {
            ControlFlow::Continue(None)
        } else {
            ControlFlow::Continue(std::mem::take(&mut self.stats))
        }
    }
}

#[inline(never)]
fn add_occurrences<'a>(
    map: &mut EntriesFound,
    found: &Finding,
    token_ref: &TokenRef,
    collect_positions: bool,
    occurrences: impl IntoIterator<Item = &'a (AttributeIndex, ValueIndex, EntryIndex, TokenPosition)>,
) {
    for (attr, value_index, entry, token_position) in occurrences {
        let positions = &mut map
            .entry(*entry)
            .or_default()
            .entry(*token_ref)
            .or_insert_with(|| MatchedIndexTerm {
                value: Value::text(found.token.as_ref()),
                score: found.score,
                positions: vec![],
            })
            .positions;
        if collect_positions {
            positions.push((*attr, *value_index, *token_position));
        }
    }
}
