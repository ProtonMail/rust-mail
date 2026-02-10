use crate::index::text::inner::TextIndex;

impl TextIndex {
    /// Returns the number of unique tokens in the index
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }
    /// Returns the number of unique trigrams in the index
    pub fn trigram_count(&self) -> usize {
        self.trigrams.len()
    }
}
