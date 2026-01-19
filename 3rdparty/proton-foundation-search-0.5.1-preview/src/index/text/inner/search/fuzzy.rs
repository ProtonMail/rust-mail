use std::collections::HashSet;

use super::*;
use crate::chunker::ChunkIter;
use crate::document::Value;
use crate::index::text::inner::filter::MatchesTextFilter;

impl TextIndex {
    /// Returns the entries matching the given term.
    /// The search is case-sensitive and works with non-ASCII characters.
    /// Text normalization needs to be done beforehand.
    #[tracing::instrument(skip_all)]
    pub(crate) fn search_matches<'a>(
        &'a self,
        filter: &MatchesTextFilter,
        attr_filter: Option<AttributeIndex>,
        universe: Option<&'a HashSet<EntryIndex>>,
    ) -> Option<(IndexSearchResults, IndexSearchStats)> {
        let matches = self.get_tokens_fuzzy(filter);
        let mut stats = IndexSearchStats::default();

        let entries = matches
            // now build the entry matches from sorted occurrences
            .into_iter()
            .flat_map(move |(token_id, (token, score))| {
                let occurrences = self
                    .get_occurrences(token_id, universe, attr_filter)
                    .chunk(|(e, _a, _v, _t)| *e, |(_e, a, v, t)| (a, v, t));

                occurrences.map(move |(entry, positions)| {
                    let matched = MatchedIndexTerm {
                        value: Value::text(token),
                        score,
                        positions: positions.collect(),
                    };

                    (token_id, entry, matched)
                })
            })
            .fold(
                BTreeMap::new(),
                |mut map: BTreeMap<EntryIndex, Vec<MatchedIndexTerm>>,
                 (token_id, entry, matched)| {
                    stats.matched(
                        entry,
                        &matched,
                        |attribute| {
                            self.get_occurrences(token_id, None, Some(attribute))
                                .count()
                        },
                        |attribute, entry| {
                            let (_length, count) =
                                self.stats.get(entry, attribute).unwrap_or_default();
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

        if entries.is_empty() {
            tracing::debug!("no matches");
            return None;
        }

        tracing::debug!(?stats, ?entries);

        Some((entries, stats))
    }

    /// get fuzzy matching token ids
    /// counts the number of trigrams matched in a bitmask per token - each 1 is one position matched
    /// Then calculates a score according to number of trigrams matched
    fn get_tokens_fuzzy(&self, filter: &MatchesTextFilter) -> HashMap<TokenRef, (&str, Score)> {
        let mut matches = HashMap::<TokenRef, [u64; 4]>::default();
        let value = filter.term.as_ref();
        let fuzz = filter.max_distance as u8;
        #[allow(clippy::expect_used)]
        let value_len = u8::try_from(value.len()).expect("too long a token / search term");
        for (pos, handle) in value.trigrams() {
            let pos = pos as u8;
            let range = TrigramPosition::new(pos.saturating_sub(fuzz))
                ..=TrigramPosition::new(pos.saturating_add(fuzz));
            let tokens = self
                .trigrams
                .get(handle)
                .map(|placements| placements.range(range))
                .into_iter()
                .flatten()
                .flat_map(|(pos, tokens)| tokens.iter().map(|token_id| (*pos, *token_id)))
                .filter(|(_pos, token_id)| {
                    let len = self.token_len(*token_id);
                    value_len.abs_diff(len) <= fuzz
                });

            for (pos, token_id) in tokens {
                let positions = matches.entry(token_id).or_default();

                // The u8 guarantees the bounds for the following calculation at compile time.
                // should that change, the calculation will need to be updated
                let pos: u8 = pos.0;
                let pos = pos as usize;

                // 4 * 64 = 256
                positions[pos / 64] |= 1 << (pos % 64);
            }
        }

        // Calculate scores for matched tokens
        let expect_chars = value.trigrams().count();
        matches.into_iter().filter_map(|(token_id, matches)| {
            let found = matches
                .iter()
                .map(|positions| positions.count_ones())
                .sum::<u32>() as usize;

            // Defensive check for data integrity issue
            if found > expect_chars {
                panic!(
                    "found {} trigrams but expected only {} for search term '{}' and token_id {:?}",
                    found, expect_chars, value, token_id
                );
            }

            let missing = expect_chars - found;
            let keep = missing < 2 + fuzz as usize;
            if !keep {
                return None;
            }

            let token = self.token(token_id);
            let score = Score::new(Self::evaluate_term_match(filter, token)?);

            let trigrams_matched = matches
                .iter()
                .map(|positions| positions.count_ones())
                .sum::<u32>();
            let accuracy = Score::new_fraction(trigrams_matched as usize, expect_chars);
            Some((token_id, (token, accuracy * score)))
        }).collect()
    }

    /// score the match according to levenshtein and thresholds
    #[instrument]
    fn evaluate_term_match(filter: &MatchesTextFilter, term: &str) -> Option<f64> {
        let MatchesTextFilter {
            term: input,
            max_distance,
            min_similarity,
        } = filter;

        let distance = distance::levenshtein(input, term);
        let score = distance::levenshtein_ratio(input, term, distance);

        if distance > *max_distance {
            tracing::trace!(distance, "term too far");
            return None;
        }
        if score < *min_similarity {
            tracing::trace!(score, "term too different");
            return None;
        }

        tracing::trace!(score, distance, "term matches");

        Some(score)
    }
}

/// Methods only meant for testing search
#[cfg(test)]
#[allow(clippy::expect_used)]
impl TextIndex {
    /// find a specific occurrence
    pub(crate) fn test_find_posting<T: AsRef<str>>(
        &self,
        term: T,
        entry_idx: EntryIndex,
        attr_idx: AttributeIndex,
        value_idx: ValueIndex,
        token_idx: TokenPosition,
    ) -> bool {
        let term = term.as_ref();
        let candidates =
            self.test_matching_trigrams(Some(attr_idx), term)
                .filter(|(e, a, v, t, ..)| {
                    *e == entry_idx && *a == attr_idx && *v == value_idx && *t == token_idx
                });
        Self::test_term_occurrences(candidates).contains_key(term)
    }

    /// Get all inserted terms
    pub fn test_get_terms(&self) -> impl Iterator<Item = String> {
        Self::test_term_occurrences(self.test_get_all_trigrams()).into_keys()
    }

    /// Get all inserted entry indices
    pub fn test_get_all_entry_ids(&self) -> impl Iterator<Item = EntryIndex> {
        self.tokens
            .iter()
            .flat_map(|(_len, occurrences)| occurrences)
            .map(|(occurrence, ..)| self.occurrence(*occurrence).0)
            .collect::<BTreeSet<_>>() // unique and sorted
            .into_iter()
    }

    /// Check if a term was inserted for entry
    pub fn test_entry_has_term<T: AsRef<str>>(&self, entry_index: EntryIndex, term: T) -> bool {
        let trigrams = self
            .test_matching_trigrams(None, term.as_ref())
            .filter(|(e, ..)| *e == entry_index);
        Self::test_term_occurrences(trigrams).contains_key(term.as_ref())
    }

    /// Retrieve all data for all entries
    pub fn test_get_all_entries(
        &self,
    ) -> BTreeMap<(EntryIndex, AttributeIndex, ValueIndex), String> {
        use itertools::Itertools as _;

        Self::test_rebuild_terms(self.test_get_all_trigrams())
            .into_iter()
            .chunk_by(|((e, a, v, _t), _term)| (*e, *a, *v))
            .into_iter()
            .fold(
                BTreeMap::<(EntryIndex, AttributeIndex, ValueIndex), String>::new(),
                |mut map, (key, terms)| {
                    let value = map.entry(key).or_default();
                    for (_, term) in terms {
                        if !value.is_empty() {
                            value.push(' ');
                        }
                        value.push_str(&term);
                    }
                    map
                },
            )
    }

    /// Get all inserted data at trigram level
    pub(crate) fn test_get_all_trigrams(
        &self,
    ) -> impl Iterator<
        Item = (
            EntryIndex,
            AttributeIndex,
            ValueIndex,
            TokenPosition,
            TrigramPosition,
            &str,
        ),
    > {
        self.trigrams
            .iter()
            .flat_map(move |(handle, placements)| {
                placements
                    .iter()
                    .map(move |(pos, tokens)| (handle, pos, tokens))
            })
            .flat_map(|(handle, pos, tokens)| {
                tokens.iter().map(move |token_id| (handle, pos, token_id))
            })
            .flat_map(move |(handle, pos, token_id)| {
                //let trigram = Self::get_cloud_trigram(&self.cloud, handle).expect("stored trigram");
                let trigram = handle.as_ref();
                self.get_occurrences(*token_id, None, None)
                    .map(move |(e, a, v, t)| (e, a, v, t, *pos, trigram))
            })
    }

    /// Searches for terms in the trigram index that match the given search term.
    ///
    /// This function breaks down the search term into trigrams and finds all terms
    /// in the index that contain these trigrams.
    ///
    /// We can filter the terms by attribute
    fn test_matching_trigrams<'a>(
        &self,
        attr_filter: Option<AttributeIndex>,
        term: &'a str,
    ) -> impl Iterator<
        Item = (
            EntryIndex,
            AttributeIndex,
            ValueIndex,
            TokenPosition,
            TrigramPosition,
            &'a str,
        ),
    > {
        term.trigrams()
            //.flat_map(|(_pos, trigram)| self.cloud.find(trigram).map(|handle| (handle, trigram)))
            .flat_map(move |(pos, trigram)| {
                let pos = TrigramPosition::new(pos as u8);
                self.trigrams
                    .get(trigram)
                    .into_iter()
                    .flat_map(move |placements| placements.get(&pos))
                    .flatten()
                    .flat_map(move |token_id| {
                        self.get_occurrences(*token_id, None, attr_filter)
                            .map(move |(e, a, v, t)| (e, a, v, t, pos, trigram))
                    })
            })
            .inspect(move |(e, a, v, t, c, trigram)| {
                tracing::trace!("term trigram {term:?} {trigram} in {e} {a} {v} {t:?} {c:?}")
            })
    }

    /// Reverse trigrams into terms
    fn test_rebuild_terms<'a>(
        from: impl Iterator<
            Item = (
                EntryIndex,
                AttributeIndex,
                ValueIndex,
                TokenPosition,
                TrigramPosition,
                &'a str,
            ),
        >,
    ) -> BTreeMap<(EntryIndex, AttributeIndex, ValueIndex, TokenPosition), String> {
        from
            // first we sort the trigrams in order of appearance int the terms
            .map(|(e, a, v, t, c, trigram)| (((e, a, v, t), c), trigram))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            // here we rebuild the terms
            .fold(
                BTreeMap::<_, String>::new(),
                |mut map, ((key, pos), trigram)| {
                    let term = map.entry(key).or_default();
                    let position = pos.offset();

                    // in case we did not match some trigrams, fill the empty space
                    let missing = position.saturating_sub(term.len());
                    term.push_str(&"*".repeat(missing));

                    // append the tail characters (assuming the head is already in place since we sort the trigrams)
                    let tail = term.len() - position;
                    term.push_str(&trigram[tail..]);

                    map
                },
            )
    }

    /// Reverse trigram serch into term matches
    fn test_term_occurrences<'a>(
        from: impl Iterator<
            Item = (
                EntryIndex,
                AttributeIndex,
                ValueIndex,
                TokenPosition,
                TrigramPosition,
                &'a str,
            ),
        >,
    ) -> BTreeMap<String, BTreeSet<(EntryIndex, AttributeIndex, ValueIndex, TokenPosition)>> {
        // first we sort the trigrams in order of appearance int the terms
        Self::test_rebuild_terms(from)
            .into_iter()
            // and invert the mapping so that term => occurrences
            .fold(
                BTreeMap::<String, BTreeSet<_>>::new(),
                |mut map, (key, term)| {
                    map.entry(term).or_default().insert(key);
                    map
                },
            )
    }

    fn test_get_trigrams(&self) -> impl Iterator<Item = &str> {
        use std::ops::Deref;
        self.trigrams.keys().map(|t| t.deref())
    }

    fn test_match_trigrams(&self, term: &str) -> f64 {
        let (count, matches) = term
            .trigrams()
            .fold((0, 0), |(count, matches), (_pos, trigram)| {
                (
                    count + 1usize,
                    matches + self.trigrams.contains_key(trigram) as usize,
                )
            });
        matches as f64 / count as f64
    }

    pub fn test_search_matches(
        &self,
        term: &str,
        max_distance: usize,
        min_similarity: f64,
    ) -> Vec<(Score, EntryIndex)> {
        use tracing::debug;

        use crate::query::expression::Operator;
        use crate::query::results::{
            FoundEntry, MatchGroup, MatchNode, MatchOccurrence, MatchValue,
        };
        use crate::query::stats::{AttributeStats, CollectionStats};

        let (results, stats) = self.search(
            &TextFilter::matches(term, max_distance, min_similarity),
            None,
            None,
        );

        debug!("{:#?}", self.stats);
        debug!("{stats:#?}");

        let stats = CollectionStats::new(stats.into_iter().map(|(attribute, attr_stats)| {
            (
                attribute.0.to_string().into_boxed_str(),
                AttributeStats::new(
                    attr_stats.entries,
                    attr_stats.size,
                    attr_stats.frequencies,
                    attr_stats
                        .sizes
                        .into_iter()
                        .map(|(entry, size)| (entry.0.to_string().into_boxed_str(), size))
                        .collect(),
                ),
            )
        }));

        debug!("{stats:#?}");

        let mut results = results
            .into_iter()
            .map(|(entry, matched)| {
                let mut found = FoundEntry::new_with_matches(
                    entry.0.to_string().into_boxed_str(),
                    MatchGroup::new(
                        Operator::Or,
                        matched.into_iter().map(|matched| {
                            MatchNode::Value(MatchValue::new(
                                matched.value,
                                matched.score,
                                matched
                                    .positions
                                    .into_iter()
                                    .map(|(a, v, p)| {
                                        MatchOccurrence::new(a.0.to_string().as_ref(), v, p)
                                    })
                                    .collect(),
                            ))
                        }),
                    ),
                );

                debug!("{found:#?}");

                stats.update_scores(&mut found);

                (
                    found.score().round(3),
                    found
                        .identifier()
                        .parse::<u32>()
                        .expect("reverse entry id")
                        .into(),
                )
            })
            .collect::<Vec<_>>();
        results.sort();
        results
    }
}

#[cfg(test)]
#[path = "tests_fuzzy.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_fuzzy_large.rs"]
mod tests_large;
