/// Index dump implementation
mod export;
/// Core text indexing implementation including token processing and trigram generation
mod inner;
/// Text search functionality including fuzzy matching, exact search, and filtering
mod search;
/// Storage and persistence layer for text indices
mod store;

use std::sync::Arc;

use inner::TextIndex;

const NAME: &str = "text";

/// A sans-io text index for now just wrapping the old TextIndex and treating it as one blob
#[derive(Debug, Default)]
pub struct TextIndexSansIo {}
