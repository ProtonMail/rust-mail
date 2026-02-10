/// WAL-based storage implementation for text indices
mod store;

pub use store::WALBasedTextIndexStore;

use crate::index::prelude::wal::*;

// Helper structures for reconstruction (copied from test)
/// Reconstructed text data for WAL operations
#[derive(Debug, Default, Clone)]
pub struct ReconstructedTextData {
    token_occurrences: std::collections::HashSet<CompositeKey>,
    trigram_mappings: std::collections::HashSet<TrigramKey>,
    token_to_ref: std::collections::HashMap<Box<str>, u64>,
}

/// Composite key for token occurrences in text index
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CompositeKey {
    /// Token reference identifier
    pub token_ref: u64,
    /// Entry index
    pub entry_index: u32,
    /// Attribute index
    pub attribute_index: u32,
    /// Value index
    pub value_index: u32,
    /// Token position within the value
    pub token_position: usize,
}

/// Key for trigram mappings in text index
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TrigramKey {
    /// The trigram string
    pub trigram: String,
    /// Position within the token
    pub position: u8,
    /// Token reference identifier
    pub token_ref: u64,
}

impl ReconstructedTextData {
    /// Create a new empty ReconstructedTextData
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a token occurrence entry to the reconstruction data
    pub fn add_token_occurrence(&mut self, entry: &TokenOccurrenceEntry) {
        let key = CompositeKey {
            token_ref: entry.token_ref,
            entry_index: entry.entry_index.0,
            attribute_index: entry.attribute_index.0 as u32,
            value_index: entry.value_index.0 as u32,
            token_position: entry.token_position,
        };
        self.token_occurrences.insert(key);
    }

    /// Remove a token occurrence entry from the reconstruction data
    pub fn remove_token_occurrence(&mut self, entry: &TokenOccurrenceEntry) {
        let key = CompositeKey {
            token_ref: entry.token_ref,
            entry_index: entry.entry_index.0,
            attribute_index: entry.attribute_index.0 as u32,
            value_index: entry.value_index.0 as u32,
            token_position: entry.token_position,
        };
        self.token_occurrences.remove(&key);
    }

    /// Add a trigram mapping entry to the reconstruction data
    pub fn add_trigram_mapping(&mut self, entry: &TrigramMappingEntry) {
        let key = TrigramKey {
            trigram: entry.trigram.clone(),
            position: entry.position,
            token_ref: entry.token_ref,
        };
        self.trigram_mappings.insert(key);
    }

    /// Remove a trigram mapping entry from the reconstruction data
    pub fn remove_trigram_mapping(&mut self, entry: &TrigramMappingEntry) {
        let key = TrigramKey {
            trigram: entry.trigram.clone(),
            position: entry.position,
            token_ref: entry.token_ref,
        };
        self.trigram_mappings.remove(&key);
    }

    /// Add a token reference entry to the reconstruction data
    pub fn add_token_ref(&mut self, entry: &TokenRefEntry) {
        self.token_to_ref
            .insert(entry.token.clone().into_boxed_str(), entry.token_ref);
    }

    /// Remove a token reference entry from the reconstruction data
    pub fn remove_token_ref(&mut self, entry: &TokenRefEntry) {
        self.token_to_ref
            .remove(&entry.token.clone().into_boxed_str());
    }

    /// Get the number of tokens in the reconstructed data
    pub fn token_count(&self) -> usize {
        self.token_to_ref.len()
    }

    /// Get a reference to the token-to-ref mapping
    pub fn get_token_to_ref(&self) -> &std::collections::HashMap<Box<str>, u64> {
        &self.token_to_ref
    }

    /// Reconstruct index data from WAL entries using iterator-driven temporal set operations
    /// This method handles complex timelines (add/remove/re-add) correctly by applying
    /// the final operation for each unique key based on timestamp ordering.
    pub fn reconstruct_from_wal_entries(&mut self, wal_entries: &[WALEntry]) {
        use std::collections::HashMap;

        use itertools::Itertools;

        // Helper function to extract CompositeKey from WAL entry
        fn extract_composite_key(
            entry: &WALEntry,
        ) -> Option<(CompositeKey, u64, WALOperationType)> {
            match entry {
                WALEntry::TokenOccurrence(e) => {
                    let key = CompositeKey {
                        token_ref: e.token_ref,
                        entry_index: e.entry_index.0,
                        attribute_index: e.attribute_index.0 as u32,
                        value_index: e.value_index.0 as u32,
                        token_position: e.token_position,
                    };
                    Some((key, e.timestamp, e.operation_type))
                }
                _ => None,
            }
        }

        // Helper function to extract TrigramKey from WAL entry
        fn extract_trigram_key(entry: &WALEntry) -> Option<(TrigramKey, u64, WALOperationType)> {
            match entry {
                WALEntry::TrigramMapping(e) => {
                    let key = TrigramKey {
                        trigram: e.trigram.clone(),
                        position: e.position,
                        token_ref: e.token_ref,
                    };
                    Some((key, e.timestamp, e.operation_type))
                }
                _ => None,
            }
        }

        // Helper function to extract token ref info from WAL entry
        fn extract_token_ref_info(
            entry: &WALEntry,
        ) -> Option<(Box<str>, u64, u64, WALOperationType)> {
            match entry {
                WALEntry::TokenRef(e) => Some((
                    e.token.clone().into_boxed_str(),
                    e.token_ref,
                    e.timestamp,
                    e.operation_type,
                )),
                _ => None,
            }
        }

        // Process token occurrences using iterator-driven temporal grouping
        let token_occurrence_ops: Vec<_> = wal_entries
            .iter()
            .filter_map(extract_composite_key)
            .collect();

        // Group by CompositeKey and sort by timestamp, then apply final operation
        let final_token_occurrences: HashMap<CompositeKey, WALOperationType> = token_occurrence_ops
            .into_iter()
            .chunk_by(|(key, _timestamp, _op_type)| key.clone())
            .into_iter()
            .filter_map(|(key, group)| {
                let mut sorted_ops: Vec<_> = group
                    .map(|(_key, timestamp, op_type)| (timestamp, op_type))
                    .collect();
                sorted_ops.sort_by_key(|(timestamp, _)| *timestamp);
                sorted_ops.last().map(|(_, op_type)| (key, *op_type))
            })
            .collect();

        // Apply final states using pure set operations
        let additions: std::collections::HashSet<CompositeKey> = final_token_occurrences
            .iter()
            .filter_map(|(key, op_type)| {
                if matches!(op_type, WALOperationType::Addition) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        let removals: std::collections::HashSet<CompositeKey> = final_token_occurrences
            .iter()
            .filter_map(|(key, op_type)| {
                if matches!(op_type, WALOperationType::Removal) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Apply using set operations
        self.token_occurrences.extend(additions);
        self.token_occurrences.retain(|key| !removals.contains(key));

        // Process trigram mappings using iterator-driven temporal grouping
        let trigram_mapping_ops: Vec<_> =
            wal_entries.iter().filter_map(extract_trigram_key).collect();

        let final_trigram_mappings: HashMap<TrigramKey, WALOperationType> = trigram_mapping_ops
            .into_iter()
            .chunk_by(|(key, _timestamp, _op_type)| key.clone())
            .into_iter()
            .filter_map(|(key, group)| {
                let mut sorted_ops: Vec<_> = group
                    .map(|(_key, timestamp, op_type)| (timestamp, op_type))
                    .collect();
                sorted_ops.sort_by_key(|(timestamp, _)| *timestamp);
                sorted_ops.last().map(|(_, op_type)| (key, *op_type))
            })
            .collect();

        // Apply final states using pure set operations
        let additions: std::collections::HashSet<TrigramKey> = final_trigram_mappings
            .iter()
            .filter_map(|(key, op_type)| {
                if matches!(op_type, WALOperationType::Addition) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        let removals: std::collections::HashSet<TrigramKey> = final_trigram_mappings
            .iter()
            .filter_map(|(key, op_type)| {
                if matches!(op_type, WALOperationType::Removal) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Apply using set operations
        self.trigram_mappings.extend(additions);
        self.trigram_mappings.retain(|key| !removals.contains(key));

        // Process token references using iterator-driven temporal grouping
        let token_ref_ops: Vec<_> = wal_entries
            .iter()
            .filter_map(extract_token_ref_info)
            .collect();

        let final_token_refs: HashMap<Box<str>, (u64, WALOperationType)> = token_ref_ops
            .into_iter()
            .chunk_by(|(token, _token_ref, _timestamp, _op_type)| token.clone())
            .into_iter()
            .filter_map(|(key, group)| {
                let mut sorted_ops: Vec<_> = group
                    .map(|(_token, token_ref, timestamp, op_type)| (timestamp, token_ref, op_type))
                    .collect();
                sorted_ops.sort_by_key(|(timestamp, _token_ref, _op_type)| *timestamp);
                sorted_ops
                    .last()
                    .map(|(_, token_ref, final_op)| (key, (*token_ref, *final_op)))
            })
            .collect();

        // Apply final states using pure set operations
        let additions: std::collections::HashMap<Box<str>, u64> = final_token_refs
            .iter()
            .filter_map(|(token, (token_ref, op_type))| {
                if matches!(op_type, WALOperationType::Addition) {
                    Some((token.clone(), *token_ref))
                } else {
                    None
                }
            })
            .collect();

        let removals: std::collections::HashSet<Box<str>> = final_token_refs
            .iter()
            .filter_map(|(token, (_, op_type))| {
                if matches!(op_type, WALOperationType::Removal) {
                    Some(token.clone())
                } else {
                    None
                }
            })
            .collect();

        // Apply using set operations
        self.token_to_ref.extend(additions);
        self.token_to_ref
            .retain(|token, _| !removals.contains(token));
    }
}
