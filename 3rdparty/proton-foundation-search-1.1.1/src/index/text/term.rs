//! Text based search term

use std::ops::RangeInclusive;

use crate::index::text::trigram::{Trigram, TrigramPosition, Trigrams};
use crate::query::expression::TermValuePart;

/// Represents a text search for a term
#[derive(Debug, Clone)]
pub struct TextTerm {
    /// The term text excluding wildcards
    pub text: Box<str>,
    /// If the term contains a wildcard
    pub is_wild: bool,
    /// List of trigrams in the term
    pub trigrams: Vec<TextTermTrigram>,
}
impl TextTerm {
    /// Extract trigrams from search term parts
    /// Returns (wildnes, minimal length, trigrams)
    pub fn new(term: &[TermValuePart]) -> Self {
        let mut length = 0;
        let mut is_wild = false;
        let mut texts = vec![];
        let trigrams = {
            // borrow tracking variables mutably so they can be moved into closures
            let length = &mut length;
            let is_wild = &mut is_wild;

            term.iter()
                .flat_map(|part| {
                    let text = match part {
                        TermValuePart::Wildcard => {
                            *is_wild = true;
                            ""
                        }
                        TermValuePart::Text(part) => part.as_ref(),
                    };
                    let new_length = *length + text.len();
                    let offset = std::mem::replace(length, new_length);
                    let is_wild = *is_wild;
                    texts.push(text);
                    part.trigrams().filter_map(move |(pos, trigram)| {
                        if text.is_empty() {
                            return None;
                        }
                        Some(TextTermTrigram {
                            is_wild,
                            position: TrigramPosition((offset + pos) as u8),
                            trigram: trigram.into(),
                        })
                    })
                })
                .collect::<Vec<_>>()
        };

        Self {
            text: texts.concat().into_boxed_str(),
            is_wild,
            trigrams,
        }
    }

    /// Get a list of lookups for trigrams
    pub fn trigram_lookup(
        &self,
        max_distance: usize,
    ) -> impl Iterator<Item = (RangeInclusive<u8>, &str)> {
        let fuzz = max_distance as u8;
        self.trigrams.iter().map(move |trigram| {
            let first = trigram.position.0.saturating_sub(fuzz);
            let last = if trigram.is_wild {
                u8::MAX
            } else {
                trigram.position.0.saturating_add(fuzz)
            };
            (first..=last, trigram.trigram.as_ref())
        })
    }
}

/// Represents a trigram of [`TextTerm`]
#[derive(Debug, Clone, Copy)]
pub struct TextTermTrigram {
    /// If the trigram follows a wildcard
    pub is_wild: bool,
    /// Bytewise offset within the term
    pub position: TrigramPosition,
    /// The particular trigram part of the term
    pub trigram: Trigram,
}
