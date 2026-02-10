//! Additive text index structures for efficient WAL merging
//!
//! This module contains the "additive" or "flat" representation of text indices,
//! designed for efficient merging of WAL (Write-Ahead Log) batches using simple
//! set operations rather than complex nested structure manipulation.

use super::{OccurrenceRef, TextIndex, TokenEntry, TokenPosition, TokenRef, TrigramPosition};
use crate::index::prelude::wal::WALEntry;
use crate::index::prelude::{AttributeIndex, EntryIndex, ValueIndex};
use crate::index::text::inner::{Stats, Trigram};

/// Reconstructed text data for WAL operations
///
/// This is the "additive" or "flat" representation of a text index that enables
/// efficient merging of WAL batches using simple set operations.
#[derive(Debug, Clone, Default)]
pub struct AdditiveTextIndex {
    /// Set of unique tokens
    tokens: std::collections::HashSet<AdditiveToken>,
    /// Set of unique occurrences
    occurrences: std::collections::HashSet<AdditiveOccurrence>,
    /// Set of unique trigrams
    trigrams: std::collections::HashSet<AdditiveTrigram>,
}

/// Additive token representation
///
/// This struct represents a token in the additive WAL format, storing
/// the token string and a reference identifier for efficient lookups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdditiveToken {
    /// The token string that was found in the text
    ///
    /// This field contains the actual text content of the token that was
    /// extracted from the document during indexing.
    token_string: String,
    /// Reference to the token in the token list
    ///
    /// This field contains a unique identifier that references the token
    /// in the internal token storage system.
    token_ref: u64,
}

/// Additive occurrence representation
///
/// This struct represents a token occurrence in the additive WAL format,
/// storing the location and context of where a token appears in a document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdditiveOccurrence {
    /// The entry index of the document containing this token
    entry_index: u32,
    /// The attribute index where this token was found
    attribute_index: u8,
    /// The value index within the attribute
    value_index: usize,
    /// The position of the token within the value
    token_position: usize,
    /// Reference to the token in the token list
    token_ref: u64,
}

/// Additive trigram representation
///
/// This struct represents a trigram in the additive WAL format,
/// storing a 3-character sequence and its position for fuzzy search.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdditiveTrigram {
    /// The trigram string (3 characters) for fuzzy matching
    trigram: String,
    /// The position of the trigram within the token
    position: u8,
    /// Reference to the token in the token list
    token_ref: u64,
}

impl AdditiveTextIndex {
    /// Get a reference to the tokens
    pub fn tokens(&self) -> &std::collections::HashSet<AdditiveToken> {
        &self.tokens
    }

    /// Get a reference to the occurrences
    pub fn occurrences(&self) -> &std::collections::HashSet<AdditiveOccurrence> {
        &self.occurrences
    }

    /// Get a reference to the trigrams
    pub fn trigrams(&self) -> &std::collections::HashSet<AdditiveTrigram> {
        &self.trigrams
    }
}

impl AdditiveToken {
    /// Get the token string
    pub fn token_string(&self) -> &str {
        &self.token_string
    }

    /// Get the token reference
    pub fn token_ref(&self) -> u64 {
        self.token_ref
    }
}

impl AdditiveOccurrence {
    /// Get the entry index
    pub fn entry_index(&self) -> u32 {
        self.entry_index
    }

    /// Get the attribute index
    pub fn attribute_index(&self) -> u8 {
        self.attribute_index
    }

    /// Get the value index
    pub fn value_index(&self) -> usize {
        self.value_index
    }

    /// Get the token position
    pub fn token_position(&self) -> usize {
        self.token_position
    }

    /// Get the token reference
    pub fn token_ref(&self) -> u64 {
        self.token_ref
    }
}

impl AdditiveTrigram {
    /// Get the trigram string
    pub fn trigram(&self) -> &str {
        &self.trigram
    }

    /// Get the position
    pub fn position(&self) -> u8 {
        self.position
    }

    /// Get the token reference
    pub fn token_ref(&self) -> u64 {
        self.token_ref
    }
}

impl AdditiveTextIndex {
    /// Merge another additive index into this one with smart remapping
    ///
    /// This handles token_ref and entry_index conflicts by remapping the incoming
    /// batch to avoid collisions with existing data.
    pub fn merge(&mut self, other: &AdditiveTextIndex) {
        // Note: Deduplication should be done at WAL level before creating AdditiveTextIndex
        // For now, merge as-is (deduplication handled upstream)

        let pre_merge_tokens = self.tokens.len();
        let pre_merge_occurrences = self.occurrences.len();
        let pre_merge_trigrams = self.trigrams.len();

        // Build token_ref remapping: other's token_ref -> self's token_ref
        let mut token_ref_mapping = std::collections::HashMap::new();
        let mut next_token_ref = self.tokens.iter().map(|t| t.token_ref).max().unwrap_or(0) + 1;

        for other_token in &other.tokens {
            // Check if we already have this token string
            if let Some(existing_token) = self
                .tokens
                .iter()
                .find(|t| t.token_string == other_token.token_string)
            {
                // Reuse existing token_ref
                token_ref_mapping.insert(other_token.token_ref, existing_token.token_ref);
            } else {
                // Assign new token_ref and add token
                let new_token_ref = next_token_ref;
                next_token_ref += 1;
                token_ref_mapping.insert(other_token.token_ref, new_token_ref);

                self.tokens.insert(AdditiveToken {
                    token_string: other_token.token_string.clone(),
                    token_ref: new_token_ref,
                });
            }
        }

        // Merge occurrences with token_ref remapping only (EntryIndex values are preserved)
        for other_occurrence in &other.occurrences {
            let remapped_token_ref = token_ref_mapping[&other_occurrence.token_ref];

            self.occurrences.insert(AdditiveOccurrence {
                entry_index: other_occurrence.entry_index, // Preserve original EntryIndex
                attribute_index: other_occurrence.attribute_index,
                value_index: other_occurrence.value_index,
                token_position: other_occurrence.token_position,
                token_ref: remapped_token_ref,
            });
        }

        // Merge trigrams with token_ref remapping
        for other_trigram in &other.trigrams {
            let remapped_token_ref = token_ref_mapping[&other_trigram.token_ref];

            self.trigrams.insert(AdditiveTrigram {
                trigram: other_trigram.trigram.clone(),
                position: other_trigram.position,
                token_ref: remapped_token_ref,
            });
        }

        let post_merge_tokens = self.tokens.len();
        let post_merge_occurrences = self.occurrences.len();
        let post_merge_trigrams = self.trigrams.len();

        tracing::info!(
            "MERGE: tokens: {}->{} (+{}), occurrences: {}->{} (+{}), trigrams: {}->{} (+{})",
            pre_merge_tokens,
            post_merge_tokens,
            post_merge_tokens - pre_merge_tokens,
            pre_merge_occurrences,
            post_merge_occurrences,
            post_merge_occurrences - pre_merge_occurrences,
            pre_merge_trigrams,
            post_merge_trigrams,
            post_merge_trigrams - pre_merge_trigrams
        );

        tracing::info!(
            "MERGE STATS: {} token_refs remapped",
            token_ref_mapping.len()
        );
    }

    /// Merge a hierarchical TextIndex into this additive index
    ///
    /// This flattens the hierarchical structure into the three relations
    /// and merges them with the existing additive data.
    pub fn merge_hierarchical(&mut self, hierarchical: &TextIndex) {
        tracing::info!(
            "🔄 FLATTENING HIERARCHICAL: {} tokens, {} trigrams",
            hierarchical.tokens.len(),
            hierarchical.trigrams.len()
        );

        let pre_merge_tokens = self.tokens.len();
        let pre_merge_occurrences = self.occurrences.len();
        let pre_merge_trigrams = self.trigrams.len();

        // Flatten tokens from hierarchical to additive
        for (token_ref, (token_string, occurrence_map)) in hierarchical.tokens.iter().enumerate() {
            let additive_token = AdditiveToken {
                token_string: token_string.to_string(),
                token_ref: token_ref as u64,
            };
            self.tokens.insert(additive_token);

            // Flatten occurrences from hierarchical to additive
            for (occurrence_ref, value_map) in occurrence_map {
                for (value_index, token_positions) in value_map {
                    for token_position in token_positions {
                        let (entry_index, attribute_index) = hierarchical
                            .occurrences
                            .get_index(occurrence_ref.0 as usize)
                            .unwrap_or(&(EntryIndex(0), AttributeIndex(0)));
                        let additive_occurrence = AdditiveOccurrence {
                            entry_index: entry_index.0,
                            attribute_index: attribute_index.0,
                            value_index: value_index.0,
                            token_position: token_position.0,
                            token_ref: token_ref as u64,
                        };
                        self.occurrences.insert(additive_occurrence);
                    }
                }
            }
        }

        // Flatten trigrams from hierarchical to additive
        for (trigram_string, positions_map) in &hierarchical.trigrams {
            for (trigram_position, token_refs) in positions_map {
                for token_ref in token_refs {
                    let additive_trigram = AdditiveTrigram {
                        trigram: trigram_string.to_string(),
                        position: trigram_position.0,
                        token_ref: token_ref.0 as u64,
                    };
                    self.trigrams.insert(additive_trigram);
                }
            }
        }

        let post_merge_tokens = self.tokens.len();
        let post_merge_occurrences = self.occurrences.len();
        let post_merge_trigrams = self.trigrams.len();

        tracing::info!(
            "🔄 HIERARCHICAL MERGE: tokens: {}->{} (+{}), occurrences: {}->{} (+{}), trigrams: {}->{} (+{})",
            pre_merge_tokens,
            post_merge_tokens,
            post_merge_tokens - pre_merge_tokens,
            pre_merge_occurrences,
            post_merge_occurrences,
            post_merge_occurrences - pre_merge_occurrences,
            pre_merge_trigrams,
            post_merge_trigrams,
            post_merge_trigrams - pre_merge_trigrams
        );
    }

    /// Convert additive form back to hierarchical TextIndex
    ///
    /// This is the "reconstruct" phase where we build the complex searchable
    /// structure from the merged flat data. The complexity is postponed until
    /// after all merging is complete.
    pub fn to_hierarchical(&self) -> TextIndex {
        tracing::info!(
            "CONVERTING TO HIERARCHICAL: {} tokens, {} occurrences, {} trigrams",
            self.tokens.len(),
            self.occurrences.len(),
            self.trigrams.len()
        );

        let mut text_index = TextIndex::default();

        // Build reverse mapping from token_ref to token_string
        let ref_to_token: std::collections::HashMap<u64, String> = self
            .tokens
            .iter()
            .map(|t| (t.token_ref, t.token_string.clone()))
            .collect();

        // Build occurrences set
        let occurrences: indexmap::IndexSet<_> = self
            .occurrences
            .iter()
            .map(|occ| {
                (
                    EntryIndex(occ.entry_index),
                    AttributeIndex(occ.attribute_index),
                )
            })
            .collect();
        text_index.occurrences = occurrences;

        // Group occurrences by token_ref
        let token_groups: std::collections::HashMap<_, Vec<_>> = self
            .occurrences
            .iter()
            .map(|occ| {
                (
                    occ.token_ref,
                    (
                        EntryIndex(occ.entry_index),
                        AttributeIndex(occ.attribute_index),
                        ValueIndex(occ.value_index),
                        TokenPosition(occ.token_position),
                    ),
                )
            })
            .fold(
                std::collections::HashMap::new(),
                |mut acc, (token_ref, data)| {
                    acc.entry(token_ref).or_default().push(data);
                    acc
                },
            );

        // Pre-build trigram structure during token processing
        let mut trigram_structure: std::collections::BTreeMap<
            Trigram,
            std::collections::BTreeMap<TrigramPosition, std::collections::BTreeSet<TokenRef>>,
        > = std::collections::BTreeMap::new();

        let mut stats = Stats::default();

        // Build token mapping and tokens list
        let mut token_ref_mapping: std::collections::HashMap<u64, TokenRef> =
            std::collections::HashMap::new();
        let mut tokens: Vec<TokenEntry> = Vec::new();

        // Sort token_refs for deterministic ordering
        let mut sorted_token_refs: Vec<_> = token_groups.keys().collect();
        sorted_token_refs.sort();

        for &token_ref in &sorted_token_refs {
            let token_string = ref_to_token
                .get(token_ref)
                .map(|s| s.to_string().into_boxed_str())
                .unwrap_or_else(|| format!("token_{token_ref:?}").into_boxed_str());

            // Group positions by occurrence -> value_index -> token_positions
            let mut occurrence_map: std::collections::BTreeMap<
                OccurrenceRef,
                std::collections::BTreeMap<ValueIndex, std::collections::BTreeSet<_>>,
            > = std::collections::BTreeMap::new();

            for (entry_index, attr_index, value_index, token_position) in &token_groups[token_ref] {
                let occurrence_ref = OccurrenceRef::from(
                    text_index
                        .occurrences_mut()
                        .get_index_of(&(*entry_index, *attr_index))
                        .unwrap_or(0),
                );

                occurrence_map
                    .entry(occurrence_ref)
                    .or_default()
                    .entry(*value_index)
                    .or_default()
                    .insert(*token_position);

                // Update entry_lengths during token processing
                stats.add(*entry_index, *attr_index, token_string.len(), 1);
            }

            let text_token_ref = TokenRef::from(tokens.len());
            token_ref_mapping.insert(*token_ref, text_token_ref);
            tokens.push((token_string.clone(), occurrence_map));
        }

        // Build trigram structure from actual trigram data (not regenerated)
        let mut processed_trigrams = 0;
        let mut skipped_trigrams = 0;
        for trigram_entry in &self.trigrams {
            let trigram_arc = trigram_entry.trigram.as_str().into();
            let trigram_pos = TrigramPosition(trigram_entry.position);

            // Map the token_ref to the new token_ref if it exists
            if let Some(&new_token_ref) = token_ref_mapping.get(&trigram_entry.token_ref) {
                trigram_structure
                    .entry(trigram_arc)
                    .or_default()
                    .entry(trigram_pos)
                    .or_default()
                    .insert(new_token_ref);
                processed_trigrams += 1;
            } else {
                skipped_trigrams += 1;
            }
        }

        tracing::info!(
            "TRIGRAM CONVERSION: processed {} trigrams, skipped {} trigrams (token_ref mapping issues)",
            processed_trigrams,
            skipped_trigrams
        );

        text_index.tokens = tokens;
        text_index.trigrams = trigram_structure;

        // Update stats
        text_index.stats = stats;

        text_index
    }

    /// Deduplicate WAL entries from a single file
    ///
    /// This function takes raw WAL entries and:
    /// 1. Consolidates duplicate TokenRef entries (same token_string)
    /// 2. Repairs broken TokenOccurrence references to use canonical token_refs
    /// 3. Repairs broken TrigramMapping references to use canonical token_refs
    /// 4. Removes duplicate trigram entries
    pub fn deduplicate_wal_entries(wal_entries: &[WALEntry]) -> Vec<WALEntry> {
        use std::collections::HashMap;

        // Step 1: Build token consolidation map
        let mut token_consolidation: HashMap<String, u64> = HashMap::new();
        let mut token_ref_remapping: HashMap<u64, u64> = HashMap::new();
        let mut canonical_tokens = Vec::new();

        // Process TokenRef entries first to build consolidation map
        for entry in wal_entries {
            if let WALEntry::TokenRef(token_ref_entry) = entry {
                if let Some(&existing_token_ref) = token_consolidation.get(&token_ref_entry.token) {
                    // We've seen this token string before - map to existing canonical token_ref
                    token_ref_remapping.insert(token_ref_entry.token_ref, existing_token_ref);
                } else {
                    // First time seeing this token string - make it canonical
                    token_consolidation
                        .insert(token_ref_entry.token.clone(), token_ref_entry.token_ref);
                    token_ref_remapping
                        .insert(token_ref_entry.token_ref, token_ref_entry.token_ref); // Identity mapping
                    canonical_tokens.push(entry.clone());
                }
            }
        }

        // Step 2: Process other entries and repair token_refs
        let mut deduplicated_entries = canonical_tokens;
        let mut seen_trigrams: std::collections::HashSet<(String, u8, u64)> =
            std::collections::HashSet::new();

        for entry in wal_entries {
            match entry {
                WALEntry::TokenRef(_) => {
                    // Already processed above
                }
                WALEntry::TokenOccurrence(occurrence_entry) => {
                    // Repair token_ref to use canonical one
                    let canonical_token_ref = token_ref_remapping
                        .get(&occurrence_entry.token_ref)
                        .copied()
                        .unwrap_or(occurrence_entry.token_ref);

                    let mut repaired_entry = occurrence_entry.clone();
                    repaired_entry.token_ref = canonical_token_ref;
                    deduplicated_entries.push(WALEntry::TokenOccurrence(repaired_entry));
                }
                WALEntry::TrigramMapping(trigram_entry) => {
                    // Repair token_ref to use canonical one
                    let canonical_token_ref = token_ref_remapping
                        .get(&trigram_entry.token_ref)
                        .copied()
                        .unwrap_or(trigram_entry.token_ref);

                    // Check for duplicate trigram (same trigram, position, canonical_token_ref)
                    let trigram_key = (
                        trigram_entry.trigram.clone(),
                        trigram_entry.position,
                        canonical_token_ref,
                    );
                    if !seen_trigrams.contains(&trigram_key) {
                        seen_trigrams.insert(trigram_key);

                        let mut repaired_entry = trigram_entry.clone();
                        repaired_entry.token_ref = canonical_token_ref;
                        deduplicated_entries.push(WALEntry::TrigramMapping(repaired_entry));
                    }
                }
                _ => {
                    // Pass through other entry types unchanged
                    deduplicated_entries.push(entry.clone());
                }
            }
        }

        deduplicated_entries
    }

    /// Create an additive index from WAL entries directly
    pub fn from_wal_entries(wal_entries: &[WALEntry]) -> Self {
        // Auto-deduplicate WAL entries before processing
        let clean_entries = Self::deduplicate_wal_entries(wal_entries);
        Self::from_wal_entries_with_offset(&clean_entries, 0)
    }

    /// Create an additive index from WAL entries with entry ID offset
    pub fn from_wal_entries_with_offset(wal_entries: &[WALEntry], entry_offset: u32) -> Self {
        let mut additive = AdditiveTextIndex::default();

        // Sort WAL entries by timestamp to ensure oldest tuples appear first
        let mut sorted_entries: Vec<_> = wal_entries.iter().collect();
        sorted_entries.sort_by_key(|entry| match entry {
            WALEntry::TokenRef(entry) => entry.timestamp,
            WALEntry::TokenOccurrence(entry) => entry.timestamp,
            WALEntry::TrigramMapping(entry) => entry.timestamp,
            WALEntry::TrivialValue(entry) => entry.timestamp,
        });

        for entry in sorted_entries {
            match entry {
                WALEntry::TokenRef(token_ref_entry) => {
                    additive.tokens.insert(AdditiveToken {
                        token_string: token_ref_entry.token.clone(),
                        token_ref: token_ref_entry.token_ref,
                    });
                }
                WALEntry::TokenOccurrence(occurrence_entry) => {
                    additive.occurrences.insert(AdditiveOccurrence {
                        entry_index: occurrence_entry.entry_index.0 + entry_offset,
                        attribute_index: occurrence_entry.attribute_index.0,
                        value_index: occurrence_entry.value_index.0,
                        token_position: occurrence_entry.token_position,
                        token_ref: occurrence_entry.token_ref,
                    });
                }
                WALEntry::TrigramMapping(trigram_entry) => {
                    additive.trigrams.insert(AdditiveTrigram {
                        trigram: trigram_entry.trigram.clone(),
                        position: trigram_entry.position,
                        token_ref: trigram_entry.token_ref,
                    });
                }
                _ => {} // Ignore other entry types
            }
        }

        additive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Removed test_merge_with_remapping - no longer relevant with Cantor pairing
    // The old test was designed to verify remapping behavior for conflicting EntryIndex values
    // With Cantor pairing, EntryIndex values are always unique across batches, so conflicts shouldn't occur

    #[test]
    fn test_merge_preserves_existing_data() {
        let mut base = AdditiveTextIndex::default();

        // Add some base data
        base.tokens.insert(AdditiveToken {
            token_string: "existing".to_string(),
            token_ref: 42,
        });
        base.occurrences.insert(AdditiveOccurrence {
            entry_index: 100,
            attribute_index: 0,
            value_index: 0,
            token_position: 0,
            token_ref: 42,
        });

        let mut incoming = AdditiveTextIndex::default();
        incoming.tokens.insert(AdditiveToken {
            token_string: "new".to_string(),
            token_ref: 0, // Will be remapped
        });
        incoming.occurrences.insert(AdditiveOccurrence {
            entry_index: 0, // Will be remapped to 101
            attribute_index: 0,
            value_index: 0,
            token_position: 0,
            token_ref: 0,
        });

        base.merge(&incoming);

        // Should have both tokens
        assert_eq!(base.tokens.len(), 2);
        assert_eq!(base.occurrences.len(), 2);

        // Original data should be unchanged
        assert!(
            base.tokens
                .iter()
                .any(|t| t.token_string == "existing" && t.token_ref == 42)
        );
        assert!(
            base.occurrences
                .iter()
                .any(|o| o.entry_index == 100 && o.token_ref == 42)
        );

        // New data should preserve its original EntryIndex (no remapping with Cantor pairing)
        assert!(base.occurrences.iter().any(|o| o.entry_index == 0)); // Preserved from 0

        println!("✅ Existing data preservation test passed!");
    }

    // Helper function to load WAL entries from file
    fn load_wal_entries_from_file(file_path: &str) -> Vec<crate::index::prelude::wal::WALEntry> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        use serde_json;

        use crate::index::prelude::wal::WALEntry;

        let mut entries = Vec::new();

        if !std::path::Path::new(file_path).exists() {
            println!("⚠️  WAL file not found: {file_path}");
            return entries;
        }

        let file = match File::open(file_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to open WAL file '{file_path}': {e}");
                return Vec::new();
            }
        };
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    eprintln!("Failed to read line: {e}");
                    continue;
                }
            };

            if line.trim().is_empty() || line.starts_with("//") {
                continue; // Skip empty lines and comments
            }

            // Parse the WAL entry
            match serde_json::from_str::<WALEntry>(&line) {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    // Skip parse errors
                }
            }
        }

        entries
    }

    // Helper function to write WAL entries to file for inspection
    fn write_wal_entries_to_file(
        entries: &[crate::index::prelude::wal::WALEntry],
        file_path: &str,
    ) {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        use serde_json;

        // Create parent directory if it doesn't exist
        #[allow(clippy::single_match)]
        match std::path::Path::new(file_path).parent() {
            Some(parent) => {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("Failed to create output directory: {e}");
                    return;
                }
            }
            None => {} // No parent directory needed
        }

        let file = match File::create(file_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to create output file '{file_path}': {e}");
                return;
            }
        };
        let mut writer = BufWriter::new(file);

        for entry in entries {
            let json_line = match serde_json::to_string(entry) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("Failed to serialize WAL entry: {e}");
                    continue;
                }
            };
            if let Err(e) = writeln!(writer, "{json_line}") {
                eprintln!("Failed to write line: {e}");
            }
        }

        if let Err(e) = writer.flush() {
            eprintln!("Failed to flush writer: {e}");
        }
    }

    // Helper function to write deduplication summary
    fn write_deduplication_summary(
        raw_entries_1: &[crate::index::prelude::wal::WALEntry],
        clean_entries_1: &[crate::index::prelude::wal::WALEntry],
        raw_entries_2: &[crate::index::prelude::wal::WALEntry],
        clean_entries_2: &[crate::index::prelude::wal::WALEntry],
        file_path: &str,
    ) {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        use crate::index::prelude::wal::WALEntry;

        // Create parent directory if it doesn't exist
        #[allow(clippy::single_match)]
        match std::path::Path::new(file_path).parent() {
            Some(parent) => {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("Failed to create output directory: {e}");
                    return;
                }
            }
            None => {} // No parent directory needed
        }

        let file = match File::create(file_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to create summary file: {e}");
                return;
            }
        };
        let mut writer = BufWriter::new(file);

        writeln!(writer, "=== WAL DEDUPLICATION SUMMARY ===\n").unwrap();

        // File 1 analysis
        let raw_margaret_1: Vec<_> = raw_entries_1
            .iter()
            .filter_map(|entry| match entry {
                WALEntry::TokenRef(token_entry)
                    if token_entry.token.to_lowercase().contains("margaret") =>
                {
                    Some(token_entry)
                }
                _ => None,
            })
            .collect();

        let clean_margaret_1: Vec<_> = clean_entries_1
            .iter()
            .filter_map(|entry| match entry {
                WALEntry::TokenRef(token_entry)
                    if token_entry.token.to_lowercase().contains("margaret") =>
                {
                    Some(token_entry)
                }
                _ => None,
            })
            .collect();

        writeln!(writer, "FILE 1 ANALYSIS:").unwrap();
        writeln!(writer, "  Raw entries: {}", raw_entries_1.len()).unwrap();
        writeln!(writer, "  Clean entries: {}", clean_entries_1.len()).unwrap();
        writeln!(
            writer,
            "  Removed: {}",
            raw_entries_1.len() - clean_entries_1.len()
        )
        .unwrap();
        writeln!(writer, "  Margaret tokens (raw): {}", raw_margaret_1.len()).unwrap();
        for token in &raw_margaret_1 {
            writeln!(
                writer,
                "    '{}' -> token_ref: {}",
                token.token, token.token_ref
            )
            .unwrap();
        }
        writeln!(
            writer,
            "  Margaret tokens (clean): {}",
            clean_margaret_1.len()
        )
        .unwrap();
        for token in &clean_margaret_1 {
            writeln!(
                writer,
                "    '{}' -> token_ref: {}",
                token.token, token.token_ref
            )
            .unwrap();
        }

        writeln!(writer).unwrap();

        // File 2 analysis
        let raw_margaret_2: Vec<_> = raw_entries_2
            .iter()
            .filter_map(|entry| match entry {
                WALEntry::TokenRef(token_entry)
                    if token_entry.token.to_lowercase().contains("margaret") =>
                {
                    Some(token_entry)
                }
                _ => None,
            })
            .collect();

        let clean_margaret_2: Vec<_> = clean_entries_2
            .iter()
            .filter_map(|entry| match entry {
                WALEntry::TokenRef(token_entry)
                    if token_entry.token.to_lowercase().contains("margaret") =>
                {
                    Some(token_entry)
                }
                _ => None,
            })
            .collect();

        writeln!(writer, "FILE 2 ANALYSIS:").unwrap();
        writeln!(writer, "  Raw entries: {}", raw_entries_2.len()).unwrap();
        writeln!(writer, "  Clean entries: {}", clean_entries_2.len()).unwrap();
        writeln!(
            writer,
            "  Removed: {}",
            raw_entries_2.len() - clean_entries_2.len()
        )
        .unwrap();
        writeln!(writer, "  Margaret tokens (raw): {}", raw_margaret_2.len()).unwrap();
        for token in &raw_margaret_2 {
            writeln!(
                writer,
                "    '{}' -> token_ref: {}",
                token.token, token.token_ref
            )
            .unwrap();
        }
        writeln!(
            writer,
            "  Margaret tokens (clean): {}",
            clean_margaret_2.len()
        )
        .unwrap();
        for token in &clean_margaret_2 {
            writeln!(
                writer,
                "    '{}' -> token_ref: {}",
                token.token, token.token_ref
            )
            .unwrap();
        }

        writeln!(writer, "\n=== SUMMARY ===").unwrap();
        writeln!(
            writer,
            "Total raw entries: {}",
            raw_entries_1.len() + raw_entries_2.len()
        )
        .unwrap();
        writeln!(
            writer,
            "Total clean entries: {}",
            clean_entries_1.len() + clean_entries_2.len()
        )
        .unwrap();
        writeln!(
            writer,
            "Total removed: {}",
            (raw_entries_1.len() - clean_entries_1.len())
                + (raw_entries_2.len() - clean_entries_2.len())
        )
        .unwrap();
        writeln!(
            writer,
            "Margaret tokens before: {}",
            raw_margaret_1.len() + raw_margaret_2.len()
        )
        .unwrap();
        writeln!(
            writer,
            "Margaret tokens after: {}",
            clean_margaret_1.len() + clean_margaret_2.len()
        )
        .unwrap();

        if let Err(e) = writer.flush() {
            eprintln!("Failed to flush summary writer: {e}");
        }
    }

    #[test]
    fn test_premerge_deduplication_with_real_wal_files() {
        // Test the premerge deduplication function with real WAL files
        let wal_files = [
            "benches/fixtures/text_val_Attribute[0]_b3.jsonl",
            "benches/fixtures/text_val_Attribute[0]_b2.jsonl",
        ];

        println!("Starting premerge deduplication test with real WAL files...");

        // Load first file and deduplicate
        println!("\n📁 Processing first WAL file: {}", wal_files[0]);
        let raw_wal_entries_1 = load_wal_entries_from_file(wal_files[0]);
        println!("  Raw entries: {}", raw_wal_entries_1.len());

        println!("🧹 Deduplicating first WAL file...");
        let clean_wal_entries_1 = AdditiveTextIndex::deduplicate_wal_entries(&raw_wal_entries_1);
        println!(
            "  Clean entries: {} (removed {} duplicates)",
            clean_wal_entries_1.len(),
            raw_wal_entries_1.len() - clean_wal_entries_1.len()
        );

        // Write deduplicated entries to file for inspection
        let output_file_1 = "tests/wal_inspection/deduplicated_file_1.jsonl";
        write_wal_entries_to_file(&clean_wal_entries_1, output_file_1);
        println!("  📝 Written deduplicated entries to: {output_file_1}");

        let index_1 = AdditiveTextIndex::from_wal_entries(&clean_wal_entries_1);
        println!(
            "  Index 1: Tokens: {}, Occurrences: {}, Trigrams: {}",
            index_1.tokens.len(),
            index_1.occurrences.len(),
            index_1.trigrams.len()
        );

        // Load second file and deduplicate
        println!("\n📁 Processing second WAL file: {}", wal_files[1]);
        let raw_wal_entries_2 = load_wal_entries_from_file(wal_files[1]);
        println!("  Raw entries: {}", raw_wal_entries_2.len());

        println!("🧹 Deduplicating second WAL file...");
        let clean_wal_entries_2 = AdditiveTextIndex::deduplicate_wal_entries(&raw_wal_entries_2);
        println!(
            "  Clean entries: {} (removed {} duplicates)",
            clean_wal_entries_2.len(),
            raw_wal_entries_2.len() - clean_wal_entries_2.len()
        );

        // Write deduplicated entries to file for inspection
        let output_file_2 = "tests/wal_inspection/deduplicated_file_2.jsonl";
        write_wal_entries_to_file(&clean_wal_entries_2, output_file_2);
        println!("  📝 Written deduplicated entries to: {output_file_2}");

        let index_2 = AdditiveTextIndex::from_wal_entries(&clean_wal_entries_2);
        println!(
            "  Index 2: Tokens: {}, Occurrences: {}, Trigrams: {}",
            index_2.tokens.len(),
            index_2.occurrences.len(),
            index_2.trigrams.len()
        );

        // Check margaret tokens in each deduplicated index - should be exactly 1 per index
        let margaret_tokens_1: Vec<_> = index_1
            .tokens
            .iter()
            .filter(|t| t.token_string.to_lowercase().contains("margaret"))
            .collect();
        let margaret_tokens_2: Vec<_> = index_2
            .tokens
            .iter()
            .filter(|t| t.token_string.to_lowercase().contains("margaret"))
            .collect();

        println!("\n🔍 MARGARET TOKEN ANALYSIS AFTER DEDUPLICATION:");
        println!("  Index 1 has {} margaret tokens:", margaret_tokens_1.len());
        for token in &margaret_tokens_1 {
            println!("    '{}' (ref: {})", token.token_string, token.token_ref);
        }
        println!("  Index 2 has {} margaret tokens:", margaret_tokens_2.len());
        for token in &margaret_tokens_2 {
            println!("    '{}' (ref: {})", token.token_string, token.token_ref);
        }

        // CRITICAL ASSERTIONS: After deduplication, each index should have exactly 1 margaret token
        assert_eq!(
            margaret_tokens_1.len(),
            1,
            "Index 1 should have exactly ONE margaret token after deduplication, found: {}",
            margaret_tokens_1.len()
        );
        assert_eq!(
            margaret_tokens_2.len(),
            1,
            "Index 2 should have exactly ONE margaret token after deduplication, found: {}",
            margaret_tokens_2.len()
        );

        // Now merge the clean indices
        println!("\n🔀 Merging deduplicated indices...");
        let mut merged_index = index_1;
        merged_index.merge(&index_2);

        println!("🏁 FINAL MERGED RESULTS:");
        println!("  Total Tokens: {}", merged_index.tokens.len());
        println!("  Total Occurrences: {}", merged_index.occurrences.len());
        println!("  Total Trigrams: {}", merged_index.trigrams.len());

        // Check margaret tokens in final merged result - should still be exactly 1
        let final_margaret_tokens: Vec<_> = merged_index
            .tokens
            .iter()
            .filter(|t| t.token_string.to_lowercase().contains("margaret"))
            .collect();

        println!("\n🔍 FINAL MARGARET TOKEN ANALYSIS:");
        println!(
            "  Final merged index has {} margaret tokens:",
            final_margaret_tokens.len()
        );
        for token in &final_margaret_tokens {
            println!("    '{}' (ref: {})", token.token_string, token.token_ref);
        }

        // ULTIMATE ASSERTION: After premerge deduplication + merge, there should be exactly 1 margaret token
        assert_eq!(
            final_margaret_tokens.len(),
            1,
            "Final merged index should have exactly ONE margaret token, found: {}",
            final_margaret_tokens.len()
        );

        // Write a summary report for human inspection
        let summary_file = "tests/wal_inspection/deduplication_summary.txt";
        write_deduplication_summary(
            &raw_wal_entries_1,
            &clean_wal_entries_1,
            &raw_wal_entries_2,
            &clean_wal_entries_2,
            summary_file,
        );
        println!("\n📊 Written deduplication summary to: {summary_file}");

        println!("\n✅ Premerge deduplication test completed successfully!");
    }

    #[test]
    fn test_three_batch_cumulative_merge() {
        // Create three simple batches with unique tokens
        let mut batch1 = AdditiveTextIndex::default();
        batch1.tokens.insert(AdditiveToken {
            token_string: "batch1_token".to_string(),
            token_ref: 1,
        });

        let mut batch2 = AdditiveTextIndex::default();
        batch2.tokens.insert(AdditiveToken {
            token_string: "batch2_token".to_string(),
            token_ref: 2,
        });

        let mut batch3 = AdditiveTextIndex::default();
        batch3.tokens.insert(AdditiveToken {
            token_string: "batch3_token".to_string(),
            token_ref: 3,
        });

        // Simulate the cumulative merge pattern
        let mut cumulative = batch1;
        println!("After batch 1: {} tokens", cumulative.tokens.len());

        cumulative.merge(&batch2);
        println!("After batch 2: {} tokens", cumulative.tokens.len());

        cumulative.merge(&batch3);
        println!("After batch 3: {} tokens", cumulative.tokens.len());

        // Should have all 3 tokens
        assert_eq!(
            cumulative.tokens.len(),
            3,
            "Should have 3 tokens after merging all batches"
        );

        // Should be able to find tokens from all batches
        assert!(
            cumulative
                .tokens
                .iter()
                .any(|t| t.token_string == "batch1_token"),
            "Should find batch1_token"
        );
        assert!(
            cumulative
                .tokens
                .iter()
                .any(|t| t.token_string == "batch2_token"),
            "Should find batch2_token"
        );
        assert!(
            cumulative
                .tokens
                .iter()
                .any(|t| t.token_string == "batch3_token"),
            "Should find batch3_token"
        );

        println!("✅ Three-batch cumulative merge test passed!");
    }
}
