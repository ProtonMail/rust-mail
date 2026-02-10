//! # Write-Ahead Log (WAL) Module
//!
//! This module provides the core WAL functionality for the search engine.
//! It enables flat index reconstruction by maintaining a log of all index operations.
//!
//! ## Key Components
//!
//! - `WALEntry`: Represents individual WAL entries for different operation types
//! - `TokenOccurrenceEntry`: Tracks token occurrences in documents
//! - `TrigramMappingEntry`: Maps trigrams to tokens for fuzzy search
//! - `TrivialValueEntry`: Records trivial value insertions (boolean, integer, tag)
//! - `TokenRefEntry`: Maps token strings to their internal references
//!
//! ## Usage
//!
//! ```rust
//! use proton_foundation_search::index::prelude::wal::{
//!     TrivialValueEntry, WALEntry, WALOperationType,
//! };
//! use proton_foundation_search::index::prelude::*;
//!
//! // Create a WAL entry for a trivial value
//! let entry = WALEntry::TrivialValue(TrivialValueEntry {
//!     entry_index: EntryIndex(0),
//!     attribute_index: AttributeIndex(1),
//!     value_index: ValueIndex(0),
//!     value: EntryValue::Integer(42),
//!     timestamp: 0,
//!     operation_type: WALOperationType::Addition,
//! });
//! ```
//!
//! ## Architecture
//!
//! WAL entries are buffered during index operations and written to storage
//! through the state machine during save operations. This provides atomic
//! persistence of both index data and WAL entries.

use serde::{Deserialize, Serialize};

use crate::index::prelude::*;

/// Type of WAL operation - distinguishes between additions and removals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WALOperationType {
    /// Document/attribute value was added
    #[default]
    Addition,
    /// Document/attribute value was removed
    Removal,
}

/// Token occurrence in a document - tracks token positions for text search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenOccurrenceEntry {
    /// The entry index of the document
    pub entry_index: EntryIndex,
    /// The attribute index where the token was found
    pub attribute_index: AttributeIndex,
    /// The value index within the attribute
    pub value_index: ValueIndex,
    /// The position of the token within the value
    pub token_position: usize,
    /// Reference to the token in the token list
    pub token_ref: u64,
    /// Unix timestamp when the entry was created
    pub timestamp: u64,
    /// Type of operation - addition or removal
    #[serde(default)]
    pub operation_type: WALOperationType,
}

/// Trigram mapping for fuzzy search - enables approximate string matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrigramMappingEntry {
    /// The trigram string (3 characters)
    pub trigram: String,
    /// The position of the trigram within the token
    pub position: u8,
    /// Reference to the token in the token list
    pub token_ref: u64,
    /// Unix timestamp when the entry was created
    pub timestamp: u64,
    /// Type of operation - addition or removal
    #[serde(default)]
    pub operation_type: WALOperationType,
}

/// Trivial value (boolean, integer, tag) in a document - simple value insertions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrivialValueEntry {
    /// The entry index of the document
    pub entry_index: EntryIndex,
    /// The attribute index where the value was found
    pub attribute_index: AttributeIndex,
    /// The value index within the attribute
    pub value_index: ValueIndex,
    /// Serialized value as a string
    pub value: EntryValue,
    /// Unix timestamp when the entry was created
    pub timestamp: u64,
    /// Type of operation - addition or removal
    #[serde(default)]
    pub operation_type: WALOperationType,
}

/// Token reference in the token list - maps token strings to their internal references
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRefEntry {
    /// The token string
    pub token: String,
    /// Reference to the token in the token list
    pub token_ref: u64,
    /// Unix timestamp when the entry was created
    pub timestamp: u64,
    /// Type of operation - addition or removal
    #[serde(default)]
    pub operation_type: WALOperationType,
}

/// WAL entry types for all index operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALEntry {
    /// Token occurrence in a document - tracks token positions for text search
    TokenOccurrence(TokenOccurrenceEntry),
    /// Trigram mapping for fuzzy search - enables approximate string matching
    TrigramMapping(TrigramMappingEntry),
    /// Trivial value (boolean, integer, tag) in a document - simple value insertions
    TrivialValue(TrivialValueEntry),
    /// Token reference in the token list - maps token strings to their internal references
    TokenRef(TokenRefEntry),
}

/// Grouped WAL entries organized by type for better structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupedWALEntries {
    /// All token reference entries
    #[serde(default)]
    pub token_refs: Vec<TokenRefEntry>,
    /// All token occurrence entries
    #[serde(default)]
    pub token_occurrences: Vec<TokenOccurrenceEntry>,
    /// All trigram mapping entries
    #[serde(default)]
    pub trigram_mappings: Vec<TrigramMappingEntry>,
    /// All trivial value entries
    #[serde(default)]
    pub trivial_values: Vec<TrivialValueEntry>,
}

/// WAL-specific load event that supports chained loading metadata
pub struct WALLoadEvent {
    /// blob name to load
    pub name: Box<str>,
    /// blob content callback with metadata support
    pub send: WALLoadCallback,
}

impl std::fmt::Debug for WALLoadEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WALLoadEvent")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// WAL load callback that supports chained loading metadata
pub type WALLoadCallback = Box<
    dyn Send
        + Sync
        + FnOnce(
            &crate::serialization::SerDes,
            Vec<u8>,
            Option<WALMetadata>,
        ) -> Result<(), Box<dyn Send + Sync + std::error::Error>>,
>;

/// Metadata for chained loading protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WALMetadata {
    /// Next timestamp in the chain (if any)
    pub next_timestamp: Option<String>,
    /// Total number of timestamps in the chain
    pub total_timestamps: usize,
    /// Current position in the chain
    pub current_position: usize,
}

/// CSV format handler for WAL entries
/// Groups entries by type to make tuple relationships obvious
#[derive(Debug, Clone)]
pub struct WALFormat;

impl WALFormat {
    /// Convert flat WAL entries to grouped structure
    pub fn group_entries(entries: &[WALEntry]) -> GroupedWALEntries {
        let mut grouped = GroupedWALEntries::default();

        for entry in entries {
            match entry {
                WALEntry::TokenRef(entry) => grouped.token_refs.push(entry.clone()),
                WALEntry::TokenOccurrence(entry) => grouped.token_occurrences.push(entry.clone()),
                WALEntry::TrigramMapping(entry) => grouped.trigram_mappings.push(entry.clone()),
                WALEntry::TrivialValue(entry) => grouped.trivial_values.push(entry.clone()),
            }
        }

        grouped
    }

    /// Convert grouped WAL entries back to flat structure
    pub fn flatten_entries(grouped: &GroupedWALEntries) -> Vec<WALEntry> {
        let mut entries = Vec::new();

        // Add token refs first (they're referenced by other entries)
        for entry in &grouped.token_refs {
            entries.push(WALEntry::TokenRef(entry.clone()));
        }

        // Add token occurrences
        for entry in &grouped.token_occurrences {
            entries.push(WALEntry::TokenOccurrence(entry.clone()));
        }

        // Add trigram mappings
        for entry in &grouped.trigram_mappings {
            entries.push(WALEntry::TrigramMapping(entry.clone()));
        }

        // Add trivial values
        for entry in &grouped.trivial_values {
            entries.push(WALEntry::TrivialValue(entry.clone()));
        }

        entries
    }

    /// Check if WAL compression is enabled
    fn is_compression_enabled() -> bool {
        std::env::var("WAL_NO_COMPRESSION").is_err()
    }

    /// Get the current WAL format name (JSONL or JSON+ZSTD)
    pub fn format_name() -> &'static str {
        if Self::is_compression_enabled() {
            "json+zstd"
        } else {
            "jsonl"
        }
    }

    /// Convert WAL entries to JSON format (compressed by default)
    /// Uses grouped structure for better organization when compressed
    /// Uses JSONL format for better readability when uncompressed
    pub fn to_json(
        entries: &[WALEntry],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if Self::is_compression_enabled() {
            // Group entries by type for better structure when compressed
            let grouped = Self::group_entries(entries);
            let json = serde_json::to_string_pretty(&grouped)?;

            // Use zstd compression (better than gzip, widely supported)
            use std::io::Write;
            let mut encoder = zstd::Encoder::new(Vec::new(), 0)?; // Level 0 = fastest
            encoder.write_all(json.as_bytes())?;
            encoder.finish().map_err(|e| e.into())
        } else {
            // Use JSONL format for better readability when uncompressed
            let mut jsonl = Vec::new();
            for entry in entries {
                let line = serde_json::to_string(entry)?;
                jsonl.extend_from_slice(line.as_bytes());
                jsonl.push(b'\n');
            }
            Ok(jsonl)
        }
    }

    /// Parse JSON format back to WAL entries (handles both compressed and uncompressed)
    /// Supports both grouped JSON and JSONL formats
    pub fn from_json(
        data: &[u8],
    ) -> Result<Vec<WALEntry>, Box<dyn std::error::Error + Send + Sync>> {
        // Try to decompress first (most common case)
        let json_data = if let Ok(decompressed) = zstd::decode_all(data) {
            String::from_utf8(decompressed)?
        } else {
            // If decompression fails, try parsing as uncompressed JSON/JSONL
            String::from_utf8(data.to_vec())?
        };

        // Try to parse as grouped format first (compressed JSON format)
        if let Ok(grouped) = serde_json::from_str::<GroupedWALEntries>(&json_data) {
            return Ok(Self::flatten_entries(&grouped));
        }

        // Try to parse as JSONL format (uncompressed, one JSON object per line)
        let mut entries = Vec::new();
        let lines: Vec<&str> = json_data
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if !lines.is_empty() {
            // Check if this looks like JSONL (multiple lines, each line is valid JSON)
            let mut is_jsonl = true;
            for line in &lines {
                if serde_json::from_str::<WALEntry>(line).is_err() {
                    is_jsonl = false;
                    break;
                }
            }

            if is_jsonl {
                for line in lines {
                    let entry = serde_json::from_str::<WALEntry>(line)?;
                    entries.push(entry);
                }
                return Ok(entries);
            }
        }

        // Fallback: try parsing as single JSON object
        Ok(serde_json::from_str(&json_data)?)
    }

    /// Append WAL metadata to data blob using simple delimiter format
    /// Format: [original_data][delimiter][metadata_json]
    pub fn append_metadata(
        data: &[u8],
        metadata: &WALMetadata,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let metadata_json = serde_json::to_string(metadata)?;
        let delimiter = b"\n---WAL-METADATA---\n";

        let mut result = Vec::with_capacity(data.len() + delimiter.len() + metadata_json.len());
        result.extend_from_slice(data);
        result.extend_from_slice(delimiter);
        result.extend_from_slice(metadata_json.as_bytes());

        Ok(result)
    }

    /// Extract WAL metadata from data blob if present
    /// Returns (data_without_metadata, metadata_if_present)
    pub fn extract_metadata(data: &[u8]) -> (Vec<u8>, Option<WALMetadata>) {
        let delimiter = b"\n---WAL-METADATA---\n";

        // Find the delimiter
        if let Some(delimiter_pos) = data
            .windows(delimiter.len())
            .position(|window| window == delimiter)
        {
            // Split the data
            let (data_part, metadata_part) = data.split_at(delimiter_pos);

            // Skip the delimiter to get just the metadata JSON
            let metadata_json = &metadata_part[delimiter.len()..];

            // Parse the metadata
            if let Ok(metadata) = serde_json::from_slice::<WALMetadata>(metadata_json) {
                return (data_part.to_vec(), Some(metadata));
            }
        }

        // No metadata found or parsing failed
        (data.to_vec(), None)
    }

    /// Extract the index type prefix from names
    pub fn extract_index_type_prefix(name: &str) -> String {
        if name.starts_with("text ") {
            "text_".to_string()
        } else if name.starts_with("u64 ") {
            "u64_".to_string()
        } else if name.starts_with("bool ") {
            "bool_".to_string()
        } else if name.starts_with("alloc::boxed::Box<str> ") {
            "str_".to_string()
        } else {
            // Fallback - extract first word and add underscore
            name.split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string()
                + "_"
        }
    }

    /// Extract revision from names like "text r1" -> Some(1)
    pub fn extract_revision_from_name(name: &str) -> Option<u64> {
        if let Some(r_part) = name.split_whitespace().last()
            && let Some(stripped) = r_part.strip_prefix('r')
        {
            return stripped.parse().ok();
        }
        None
    }
}
