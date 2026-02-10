/// Index dump implementation
mod export;
/// Core text indexing implementation including token processing and trigram generation
mod inner;
pub mod processor;
/// Text search functionality including fuzzy matching, exact search, and filtering
mod search;
/// Storage and persistence layer for text indices
mod store;
/// Trigram generation and processing for fuzzy text search
pub mod trigram;
/// Write-Ahead Log (WAL) integration for text indices
pub mod wal;

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use inner::TextIndex;
pub use search::filter::TextSearch;

const NAME: &str = "text";

/// A sans-io text index for now just wrapping the old TextIndex and treating it as one blob
#[derive(Debug, Default)]
pub struct TextIndexSansIo {
    reader: Arc<ArcSwapOption<(u64, TextIndex)>>,
    writer: Arc<ArcSwapOption<(u64, TextIndex)>>,
}
