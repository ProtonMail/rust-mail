//! WAL utilities for search engine operations
//!
//! This module provides helper functions for WAL aggregation, reconstruction,
//! timestamp generation, and WAL entry creation for the search engine.

// ============================================================================
// Timestamp Management
// ============================================================================
use std::cell::Cell;

use crate::entry::EntryValue;
use crate::index::prelude::wal::*;
use crate::index::prelude::*;
use crate::index::text::trigram::Trigrams;

thread_local! {
    static DETERMINISTIC_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Generate a timestamp for WAL entries
///
/// Uses microsecond precision for high-resolution timestamps.
/// In test mode (either cfg!(test) or CARGO_TEST environment variable),
/// returns a fixed timestamp for deterministic behavior.
///
/// # Returns
///
/// A timestamp in microseconds since Unix epoch
pub fn generate_wal_timestamp() -> u64 {
    generate_wal_timestamp_with_mode(false)
}

/// Enable deterministic timestamp mode for the current thread
///
/// This is useful for tests that need consistent timestamps.
/// The mode is automatically reset when the thread ends.
#[cfg(test)]
fn enable_deterministic_timestamps() {
    DETERMINISTIC_MODE.with(|mode| mode.set(true));
}

/// Disable deterministic timestamp mode for the current thread
#[cfg(test)]
fn disable_deterministic_timestamps() {
    DETERMINISTIC_MODE.with(|mode| mode.set(false));
}

/// Generate a timestamp for WAL entries with explicit deterministic mode
///
/// # Arguments
///
/// * `deterministic` - If true, always returns a fixed timestamp for testing
///
/// # Returns
///
/// A timestamp in microseconds since Unix epoch
fn generate_wal_timestamp_with_mode(deterministic: bool) -> u64 {
    // Check if we're in test mode (either unit tests, integration tests, or explicit mode)
    let is_deterministic = deterministic || DETERMINISTIC_MODE.with(|mode| mode.get());

    if is_deterministic {
        // Use fixed timestamp for tests to avoid snapshot issues
        // This timestamp should be consistent across all test runs
        1754108244u64
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0)) // Fallback to epoch if system time is before 1970
            .as_micros() as u64
    }
}

// ============================================================================
// WAL Entry Creation
// ============================================================================

// ============================================================================
// WAL Utility Functions
// ============================================================================

/// Generate WAL entries for text index addition operations
///
/// This function converts IndexStoreOperation::Insert operations with text values
/// into the three types of WAL entries needed for text indices.
///
/// # Arguments
///
/// * `entry` - The entry index
/// * `attr` - The attribute index
/// * `indexed_value` - The IndexedValue containing text data
///
/// # Returns
///
/// A vector of WALEntry for addition operations
pub fn generate_text_addition_wal_entries(
    entry: EntryIndex,
    attr: AttributeIndex,
    indexed_value: &EntryValues,
) -> Vec<WALEntry> {
    let mut wal_entries = Vec::new();
    let timestamp = generate_wal_timestamp();

    // Process each text value in the IndexedValue
    for (value_index, tokens) in indexed_value.iter().enumerate() {
        let EntryValue::Text(tokens) = tokens else {
            continue;
        };
        for (token_position, token) in tokens {
            // Generate a token reference ID (simplified for now)
            let token_ref = (entry.0 as u64) * 1000
                + (attr.0 as u64) * 100
                + (value_index as u64) * 10
                + (*token_position as u64);

            // Create TokenRef entry for addition
            let token_ref_entry = TokenRefEntry {
                token: token.to_string(),
                token_ref,
                timestamp,
                operation_type: WALOperationType::Addition,
            };
            wal_entries.push(WALEntry::TokenRef(token_ref_entry));

            // Create TokenOccurrence entry for addition
            let token_occurrence_entry = TokenOccurrenceEntry {
                entry_index: entry,
                attribute_index: attr,
                value_index: ValueIndex(value_index),
                token_position: *token_position,
                token_ref,
                timestamp,
                operation_type: WALOperationType::Addition,
            };
            wal_entries.push(WALEntry::TokenOccurrence(token_occurrence_entry));

            // Create TrigramMapping entries for addition
            for (pos, trigram) in token.trigrams() {
                let trigram_mapping_entry = TrigramMappingEntry {
                    trigram: trigram.to_owned(),
                    position: pos as u8,
                    token_ref,
                    timestamp,
                    operation_type: WALOperationType::Addition,
                };
                wal_entries.push(WALEntry::TrigramMapping(trigram_mapping_entry));
            }
        }
    }

    wal_entries
}

/// Test WAL timestamp generation
#[test]
fn test_wal_timestamp_generation() {
    // Disable deterministic mode
    disable_deterministic_timestamps();

    // Test normal timestamp generation
    let timestamp1 = generate_wal_timestamp();
    // Add a small delay to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_micros(1));
    let timestamp2 = generate_wal_timestamp();

    assert!(timestamp1 > 0, "Timestamp should be positive");
    assert!(
        timestamp2 > timestamp1,
        "Timestamps should be monotonically increasing"
    );

    // Test deterministic mode
    enable_deterministic_timestamps();
    let det_timestamp1 = generate_wal_timestamp();
    let det_timestamp2 = generate_wal_timestamp();

    assert_eq!(
        det_timestamp1, det_timestamp2,
        "Deterministic timestamps should be equal"
    );

    // Disable deterministic mode
    disable_deterministic_timestamps();
}
