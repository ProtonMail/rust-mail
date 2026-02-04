//! Index filters.

use crate::query::expression::TermValue;

/// Applies a fuzzy matching constraint against an attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchesTextFilter {
    /// The filter's search term.
    pub term: TermValue,
    /// The maximum allowed absolute edit distance.
    ///
    /// A value of `usize::MAX` accepts all entries,
    /// a value of `0` only accepts exact matches,
    /// everything in between accepts fuzzy matches.
    pub max_distance: usize,
    /// The minimum required relative similarity score.
    ///
    /// A value of `0.0` accepts all entries,
    /// a value of `1.0` only accepts exact matches,
    /// everything in between accepts fuzzy matches.
    pub min_similarity: f64,
}

impl MatchesTextFilter {
    /// Creates a filter for the provided `term`.
    pub fn new(term: TermValue, max_distance: usize, min_similarity: f64) -> Self {
        Self {
            term,
            max_distance,
            min_similarity,
        }
    }
}

/// Applies a strict matching constraint against an attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqualsTextFilter {
    /// The filter's search term.
    pub term: TermValue,
}

impl EqualsTextFilter {
    /// Creates a filter for the provided `term`.
    pub fn new(term: TermValue) -> Self {
        Self { term }
    }
}

/// Representation of multiple filter operations on the text index.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFilter {
    /// Applies a fuzzy matching constraint against an attribute.
    Matches(MatchesTextFilter),
    /// Applies a strict matching constraint against an attribute.
    Equals(EqualsTextFilter),
}

// A set of shorthands handling conversion.
impl TextFilter {
    /// Creates a fuzzy matching constraint.
    #[inline]
    pub fn matches(value: TermValue, max_distance: usize, min_similarity: f64) -> Self {
        Self::Matches(MatchesTextFilter::new(value, max_distance, min_similarity))
    }

    /// Creates a strict matching constraint.
    #[inline]
    pub fn equals(value: TermValue) -> Self {
        Self::Equals(EqualsTextFilter::new(value))
    }
}
