//! WAL-based index management system
//!
//! This module provides a complete replacement for the transaction-based index management,
//! using Write-Ahead Log (WAL) for state persistence and reconstruction.

/// Re-export WAL-based store implementations
pub mod stores {
    pub use crate::index::text::wal::WALBasedTextIndexStore;
    pub use crate::index::trivial::wal::WALBasedTrivialIndexStore;
}

/// Re-export WAL-based store types for convenience
pub use stores::*;
