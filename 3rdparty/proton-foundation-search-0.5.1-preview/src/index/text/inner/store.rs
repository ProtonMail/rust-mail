use indexmap::set::MutableValues;
use tracing::{debug, error};

use super::*;

// Type alias for removed tokens result map
type RemovedTokensResult =
    HashMap<(EntryIndex, AttributeIndex), BTreeMap<ValueIndex, BTreeMap<TokenPosition, Box<str>>>>;

impl TextIndex {
    /// Removes entries from the index and updates all related data structures
    ///
    /// This method removes the specified entries and updates:
    /// - Token occurrences
    /// - Trigram mappings
    /// - Statistics
    /// - Entry length tracking
    ///
    /// # Arguments
    ///
    /// * `entries` - Set of entry indices to remove from the index
    ///
    /// # Returns
    ///
    /// Vector of removed data tuples: (EntryIndex, AttributeIndex, IndexedValue)
    pub fn remove(
        &mut self,
        entries: &BTreeSet<EntryIndex>,
    ) -> Vec<(EntryIndex, AttributeIndex, EntryValues)> {
        self.remove_inner(entries, None)
    }

    /// this method is called as part of defragmentation. It is an opportunity to defragment the cloud as well
    pub fn remap(&mut self, mapping: &std::collections::BTreeMap<EntryIndex, EntryIndex>) -> bool {
        if mapping.is_empty() {
            return false;
        }

        let mut remapped = false;

        // using retain2 as there is no mutable iterator
        self.occurrences.retain2(|(e, a)| {
            if let Some(new_entry_idx) = mapping.get(e) {
                remapped = true;
                let (length, count) = self.stats.remove(*e, *a);
                self.stats.set(*new_entry_idx, *a, length, count);
                *e = *new_entry_idx;
            }
            true
        });

        debug!(
            message = "remapped {remapped}",
            trigrams = self.trigrams.len()
        );
        remapped
    }

    /// Insert tokens into the index
    #[tracing::instrument(skip(self))]
    pub fn insert(
        &mut self,
        entry_index: EntryIndex,
        attribute_index: AttributeIndex,
        value: &EntryValues,
    ) -> bool {
        // Let's not leave dangling trigrams and tokens behind.
        // Inserting tokens over previous ones would displace previous trigrams and tokens that differ
        // without any means to clean them up.
        // To be sure, we first remove any previously inserted tokens for this occurrence.
        // But that would misreport on modification trackinng, if the removed and inserted values
        // were the same, so we have to adjust for that.
        let removed_values = self.remove_inner(&[entry_index].into(), Some(attribute_index));
        let removed = removed_values
            .iter()
            .flat_map(|(e, a, v)| {
                assert_eq!(*e, entry_index);
                assert_eq!(*a, attribute_index);
                v.iter().enumerate()
            })
            .collect::<BTreeSet<_>>();

        let mut inserted = BTreeSet::new();
        for (value_index, value) in value.iter().enumerate() {
            let EntryValue::Text(tokens) = value else {
                continue;
            };
            let mut modified = false;

            for (token_index, token) in tokens {
                modified |= self.insert_token(
                    entry_index,
                    attribute_index,
                    value_index.into(),
                    (*token_index).into(),
                    token,
                );
            }
            if modified {
                inserted.insert((value_index, value));
            }
        }
        // The index is modified when there are any diffs between removed and inserted
        let modified = removed.symmetric_difference(&inserted).next().is_some();
        tracing::trace!(modified, ?removed, ?inserted);
        modified
    }
}

impl TextIndex {
    fn insert_token<T: AsRef<str>>(
        &mut self,
        entry_index: EntryIndex,
        attribute_index: AttributeIndex,
        value_index: ValueIndex,
        token_index: TokenPosition,
        token: T,
    ) -> bool {
        let token = token.as_ref();
        if token.len() > u8::MAX as usize {
            error!("token too long");
            return false;
        }

        let token_ids = self.get_token_ids_exact(token, false);

        let single = token_ids.len() == 1;
        let token_id = token_ids.iter().copied().next();
        assert!(
            single || token_id.is_none(),
            "if we have a token match for {token:?}, it must be unique - {token_id:?} vs {token_ids:?},\n{self:#?}",
        );

        // use existing token if found
        let token_id = token_id.unwrap_or_else(|| {
            // or insert a token
            let id = TokenRef::from(self.tokens.len());
            self.tokens.push((token.into(), Default::default()));
            id
        });

        let (occurrence, mut inserted) =
            self.occurrences.insert_full((entry_index, attribute_index));

        inserted |= self
            .token_occurrences_mut(token_id)
            .entry(OccurrenceRef::from(occurrence))
            .or_default()
            .entry(value_index)
            .or_default()
            .insert(token_index);

        for (pos, trigram) in token.trigrams() {
            inserted |= self
                .trigrams
                .entry(trigram.into())
                .or_default()
                .entry(TrigramPosition::new(pos as u8))
                .or_default()
                .insert(token_id)
        }

        if inserted {
            self.stats.add(entry_index, attribute_index, token.len(), 1);
        }

        inserted
    }

    /// Moves multiple entries from the index in a single bulk operation.
    /// This is much more efficient than calling move_entry() multiple times.
    /// Very tricky as TokenRef is by virtue of its loaded position
    pub fn remove_inner(
        &mut self,
        entries: &BTreeSet<EntryIndex>,
        attr_filter: Option<AttributeIndex>,
    ) -> Vec<(EntryIndex, AttributeIndex, EntryValues)> {
        if entries.is_empty() {
            return Vec::new();
        }

        /*
         * The text index is made up of three main data structures:
         *   1. occurrences: Tracks which (entry, attribute, value) triples exist in the index.
         *   2. tokens: Stores each unique token (word) and, for each, a mapping of which occurrences and positions it appears in.
         *   3. trigrams: Maps character trigrams to the tokens that contain them, for fast fuzzy search.
         *
         * When moving entries in bulk, we must remove all traces of those entries from all three structures.
         * However, as we remove occurrences and tokens, the underlying vectors and maps are mutated and compacted.
         * This means that token references (TokenRef) may no longer point to the same string after mutation.
         *
         * To ensure we can reconstruct the correct tokens for the entries being moved, we capture a mapping
         * from TokenRef to the token string BEFORE any mutation occurs. This allows us to accurately
         * reconstruct the original tokens for the moved entries, even after the index has been compacted.
         */

        // 1: Collect all occurrences to be removed in a single pass
        // The occurrences list contains (EntryIndex, AttributeIndex, ValueIndex) tuples
        // We need to identify which occurrences belong to entries we're moving
        let mut occurrence_ref = OccurrenceRef::new(0);
        let mut removed_occurrences = BTreeMap::new();

        // retain2() removes matching occurrences and keeps non-matching ones
        // For each occurrence, check if its EntryIndex is in our move set
        // and matches attribute filter
        self.occurrences.retain2(|(e, a)| {
            let matched = entries.contains(e)
                && attr_filter
                    .map(|attr_filter| *a == attr_filter)
                    .unwrap_or(true);
            if matched {
                // This occurrence belongs to an entry we're moving
                // Store it with its occurrence reference for later processing
                removed_occurrences.insert(occurrence_ref, (*e, *a));
            }
            // Increment occurrence reference for the next occurrence
            occurrence_ref = OccurrenceRef::from(occurrence_ref.offset() + 1);
            // Return false to remove matched occurrences, true to keep others
            !matched
        });

        if removed_occurrences.is_empty() {
            return Vec::new();
        }

        // 2: Process tokens in a single pass
        // Each token has a list of occurrences where it appears
        // We need to remove occurrences for moved entries and update references for remaining ones
        let mut removed_tokens: BTreeMap<TokenRef, Vec<_>> = BTreeMap::new();
        let mut remap_tokens = BTreeSet::default();
        let mut token_ref = TokenRef::new(0);
        // Capture token strings before any mutation to self.tokens
        let token_strings: HashMap<TokenRef, Box<str>> = self
            .tokens
            .iter()
            .enumerate()
            .map(|(i, (token, _))| (TokenRef::new(i as u32), token.clone()))
            .collect();

        // Process each token and its occurrences
        self.tokens.retain_mut(|(token, occurrences)| {
            // Take ownership of the occurrences list to modify it
            *occurrences = std::mem::take(occurrences)
                .into_iter()
                .filter_map(|(mut occurrence, tokens)| {
                    if removed_occurrences.contains_key(&occurrence) {
                        // This occurrence belongs to an entry we're moving
                        // Reduce the entry length stats for the removed entry
                        let (entry, attr) = removed_occurrences[&occurrence];
                        let count = tokens
                            .values()
                            .map(|positions| positions.len())
                            .sum::<usize>();
                        let len = token.len();
                        self.stats.sub(entry, attr, len, count);

                        // Collect this occurrence for removal
                        removed_tokens
                            .entry(token_ref)
                            .or_default()
                            .push((occurrence, tokens));
                        None // Remove this occurrence
                    } else {
                        // This occurrence stays, but we need to update its reference
                        // because some occurrences before it were removed
                        occurrence -= removed_occurrences
                            .keys()
                            .take_while(|o| **o < occurrence)
                            .count();
                        Some((occurrence, tokens)) // Keep this occurrence with updated reference
                    }
                })
                .collect();

            // Keep this token if it still has any occurrences
            let keep = !occurrences.is_empty();
            if !keep {
                // This token has no more occurrences, mark it for removal from trigrams
                remap_tokens.insert(token_ref);
            }
            token_ref = TokenRef::from(token_ref.offset() + 1);
            keep
        });

        // 3: Process trigrams in a single pass
        // Trigrams map character sequences to tokens that contain them
        // We need to remove references to tokens that no longer exist

        // Process each trigram and its token placements
        self.trigrams.retain(|_handle, placements| {
            placements.retain(|_pos, tokens| {
                // Take ownership of the tokens list to modify it
                *tokens = std::mem::take(tokens)
                    .into_iter()
                    .filter_map(|mut token_ref| {
                        if remap_tokens.contains(&token_ref) {
                            // This token has no more occurrences, remove it
                            None
                        } else {
                            // This token stays, but we need to update its reference
                            // because some tokens before it were removed
                            token_ref -=
                                remap_tokens.iter().take_while(|t| **t < token_ref).count();
                            Some(token_ref)
                        }
                    })
                    .collect();
                // Keep this placement if it still has any tokens
                !tokens.is_empty()
            });
            // Keep this trigram if it still has any placements
            !placements.is_empty()
        });

        // 5: Return the removed data in the expected format
        // We need to reconstruct the (EntryIndex, AttributeIndex, ValueIndex, IndexedValue) mapping
        // from the removed tokens and their occurrences
        let mut result: RemovedTokensResult = HashMap::new();

        // Create a reverse mapping from occurrence to its tokens
        let mut occurrence_to_tokens: HashMap<
            OccurrenceRef,
            Vec<(ValueIndex, TokenPosition, Box<str>)>,
        > = HashMap::new();

        // For each removed token and its occurrences
        for (token_ref, occurrences) in removed_tokens {
            let token_string = token_strings.get(&token_ref).cloned().unwrap_or_default();
            for (occurrence, tokens) in occurrences {
                for (value_idx, tokens) in tokens {
                    for token_pos in tokens {
                        occurrence_to_tokens.entry(occurrence).or_default().push((
                            value_idx,
                            token_pos,
                            token_string.clone(),
                        ));
                    }
                }
            }
        }

        // Next reconstruct the result by occurrence (which maps to a unique entry, attr, value)
        for (occurrence, tokens) in occurrence_to_tokens {
            let occurrence_ref = removed_occurrences[&occurrence];
            let entry = result.entry(occurrence_ref).or_default();

            for (value_idx, token_pos, token_string) in tokens {
                entry
                    .entry(value_idx)
                    .or_default()
                    .insert(token_pos, token_string);
            }
        }

        // Convert the grouped tokens into the expected return format
        result
            .into_iter()
            .map(|((entry_idx, a), values)| {
                (
                    entry_idx,
                    a,
                    values
                        .into_values()
                        .map(|tokens| {
                            tokens
                                .into_iter()
                                .map(|(TokenPosition(pos), token)| (pos, token))
                                .collect::<Vec<_>>()
                                .into()
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }
}

impl From<Vec<(usize, Box<str>)>> for EntryValue {
    fn from(value: Vec<(usize, Box<str>)>) -> Self {
        EntryValue::Text(value)
    }
}

#[cfg(test)]
#[path = "tests_store.rs"]
mod tests_store;
