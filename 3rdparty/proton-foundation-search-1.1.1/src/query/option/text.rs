//! Query options for text based search

use std::ops::{Deref, DerefMut};

use super::QueryOption;
use crate::query::option::QueryOptions;

/// The default minimum required relative similarity score.
const DEFAULT_MIN_SIMILARITY: f64 = 0.75;

/// The default maximum allowed absolute edit distance.
const DEFAULT_MAX_DISTANCE: usize = 3;

/// The minimum levenshtein based similarity between the search term and matched token
///
/// Example:
///
/// ```rust
/// use proton_foundation_search::query::option::QueryOptions;
/// use proton_foundation_search::query::option::text::MinimumSimilarity;
/// let opt = QueryOptions::default().with::<MinimumSimilarity>(|value| **value = 0.5);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MinimumSimilarity(f64);
impl MinimumSimilarity {
    /// Get the option value
    pub fn get(options: &QueryOptions) -> f64 {
        *options.get::<Self>().copied().unwrap_or_default()
    }
}
impl QueryOption for MinimumSimilarity {}
impl Default for MinimumSimilarity {
    fn default() -> Self {
        DEFAULT_MIN_SIMILARITY.into()
    }
}
impl Deref for MinimumSimilarity {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for MinimumSimilarity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<f64> for MinimumSimilarity {
    fn from(value: f64) -> Self {
        Self(value)
    }
}
impl From<MinimumSimilarity> for f64 {
    fn from(value: MinimumSimilarity) -> Self {
        value.0
    }
}

/// The maximum levenshtein based distance between the search term and matched token
///
/// Example:
///
/// ```rust
/// use proton_foundation_search::query::option::QueryOptions;
/// use proton_foundation_search::query::option::text::MaximumDistance;
/// let opt = QueryOptions::default().with::<MaximumDistance>(|value| **value = 3);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MaximumDistance(usize);
impl MaximumDistance {
    /// Get the option value
    pub fn get(options: &QueryOptions) -> usize {
        *options.get::<Self>().copied().unwrap_or_default()
    }
}
impl QueryOption for MaximumDistance {}
impl Default for MaximumDistance {
    fn default() -> Self {
        DEFAULT_MAX_DISTANCE.into()
    }
}
impl Deref for MaximumDistance {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for MaximumDistance {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<usize> for MaximumDistance {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl From<MaximumDistance> for usize {
    fn from(value: MaximumDistance) -> Self {
        value.0
    }
}

#[test]
fn difference() {
    let _opt = super::QueryOptions::default().with::<MinimumSimilarity>(|value| **value = 0.5);
}

#[test]
fn distance() {
    let _opt = super::QueryOptions::default().with::<MaximumDistance>(|value| **value = 3);
}
