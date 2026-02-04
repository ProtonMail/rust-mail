//! Structure and methods for keeping track of matched tokens

use tracing::{instrument, trace};

use crate::index::text::distance;
use crate::index::text::term::TextTerm;
use crate::index::text::token::Token;
use crate::query::results::Score;

/// Tracks progress on matched token
#[derive(Debug, Default)]
pub struct Finding {
    /// Token in question
    pub token: Token,
    /// bit map of matched trigrams
    pub matches: [u64; 4],
    /// final score for the finding
    pub score: Score,
}

impl Finding {
    /// Create a new finding
    pub fn new(token: Token) -> Self {
        Self {
            token,
            matches: Default::default(),
            score: Default::default(),
        }
    }

    /// Check if the finding passes requirements and update the score
    #[instrument]
    pub fn satisfy(&mut self, term: &TextTerm, max_distance: usize, min_similarity: f64) -> bool {
        if !term.is_wild || max_distance == 0 {
            let expected_trigrams = term.trigrams.len() as u32;
            let matches = self
                .matches
                .map(|bits| bits.count_ones())
                .iter()
                .sum::<u32>();
            if max_distance == 0 {
                // Exact match, all trigrams must match
                if expected_trigrams != matches {
                    return false;
                }
            } else if expected_trigrams.saturating_sub(matches) > 2 + max_distance as u32 {
                // fuzzy non-wild mismatch
                return false;
            }
        }

        let distance = distance::levenshtein(&term.text, &self.token);
        let similarity = distance::ratio(term.text.len(), self.token.len(), distance as f64);

        trace!(?distance, ?similarity);

        if !term.is_wild && (distance > max_distance || similarity < min_similarity) {
            return false;
        }

        self.score = Score::new(similarity);

        true
    }
}
