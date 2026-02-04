//! Trigram generation and processing for fuzzy text search

#[allow(clippy::module_inception, reason = "refactoring")]
mod trigram;
mod trigrams;

use serde::{Deserialize, Serialize};
pub use trigram::*;
pub use trigrams::*;

/// The position of a trigram within a token - counted by bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrigramPosition(pub u8);

impl TrigramPosition {
    /// Creates a new TrigramPosition.
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the offset as usize.
    pub(crate) fn offset(&self) -> usize {
        self.0 as usize
    }
}
impl From<u8> for TrigramPosition {
    fn from(value: u8) -> Self {
        Self(value)
    }
}
