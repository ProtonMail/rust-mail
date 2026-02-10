//! Core search engine types
//!
//! This crate provides local search for both message bodies and metadata.

/// Result of an index operation
#[derive(Debug, Clone)]
pub struct IndexResult {
    /// Whether cleanup is needed after this operation
    pub cleanup_needed: bool,
}

impl IndexResult {
    /// Create result indicating cleanup is needed
    #[must_use]
    pub fn needs_cleanup() -> Self {
        Self {
            cleanup_needed: true,
        }
    }

    /// Create result indicating no cleanup needed
    #[must_use]
    pub fn no_cleanup() -> Self {
        Self {
            cleanup_needed: false,
        }
    }
}

/// Result of a cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// Number of blobs deleted
    pub blobs_deleted: usize,
}

/// Statistics about the search index
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    /// Total number of documents in the index
    pub documents_total: usize,
    /// Whether the engine is currently writing
    pub is_writing: bool,
}
