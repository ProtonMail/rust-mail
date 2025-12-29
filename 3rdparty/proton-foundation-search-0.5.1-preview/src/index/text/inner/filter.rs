//! Index filters.

/// Applies a prefix constraint against an attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartsWithTextFilter {
    /// The filter's search term prefix.
    pub prefix: Box<str>,
}

impl StartsWithTextFilter {
    /// Creates a filter for the provided `prefix`.
    pub fn new<T>(prefix: T) -> Self
    where
        T: AsRef<str>,
    {
        Self {
            prefix: prefix.as_ref().into(),
        }
    }
}
impl AsRef<Box<str>> for StartsWithTextFilter {
    fn as_ref(&self) -> &Box<str> {
        &self.prefix
    }
}
impl AsMut<Box<str>> for StartsWithTextFilter {
    fn as_mut(&mut self) -> &mut Box<str> {
        &mut self.prefix
    }
}

/// Applies a fuzzy matching constraint against an attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchesTextFilter {
    /// The filter's search term.
    pub term: Box<str>,
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
    pub fn new<T>(term: T, max_distance: usize, min_similarity: f64) -> Self
    where
        T: AsRef<str>,
    {
        Self {
            term: term.as_ref().into(),
            max_distance,
            min_similarity,
        }
    }

    /// Sets the filter's maximum distance threshold.
    pub fn max_distance(mut self, max_distance: usize) -> Self {
        self.max_distance = max_distance;
        self
    }

    /// Sets the filter's minimum similarity threshold.
    pub fn min_similarity(mut self, min_similarity: f64) -> Self {
        debug_assert!(min_similarity >= 0.0);
        debug_assert!(min_similarity <= 1.0);

        self.min_similarity = min_similarity.clamp(0.0, 1.0);
        self
    }
}
impl AsRef<Box<str>> for MatchesTextFilter {
    fn as_ref(&self) -> &Box<str> {
        &self.term
    }
}
impl AsMut<Box<str>> for MatchesTextFilter {
    fn as_mut(&mut self) -> &mut Box<str> {
        &mut self.term
    }
}

/// Applies a strict matching constraint against an attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqualsTextFilter {
    /// The filter's search term.
    pub term: Box<str>,
}

impl EqualsTextFilter {
    /// Creates a filter for the provided `term`.
    pub fn new<T>(term: T) -> Self
    where
        T: AsRef<str>,
    {
        Self {
            term: term.as_ref().into(),
        }
    }
}
impl AsRef<Box<str>> for EqualsTextFilter {
    fn as_ref(&self) -> &Box<str> {
        &self.term
    }
}
impl AsMut<Box<str>> for EqualsTextFilter {
    fn as_mut(&mut self) -> &mut Box<str> {
        &mut self.term
    }
}

/// Representation of multiple filter operations on the text index.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFilter {
    /// Applies a prefix constraint against an attribute.
    StartsWith(StartsWithTextFilter),
    /// Applies a fuzzy matching constraint against an attribute.
    Matches(MatchesTextFilter),
    /// Applies a strict matching constraint against an attribute.
    Equals(EqualsTextFilter),
}

// A set of shorthands handling conversion.
impl TextFilter {
    /// Creates a prefix constraint.
    #[inline]
    pub fn starts_with<V>(value: V) -> Self
    where
        V: AsRef<str>,
    {
        Self::StartsWith(StartsWithTextFilter::new(value))
    }

    /// Creates a fuzzy matching constraint.
    #[inline]
    pub fn matches<V>(value: V, max_distance: usize, min_similarity: f64) -> Self
    where
        V: AsRef<str>,
    {
        Self::Matches(MatchesTextFilter::new(value, max_distance, min_similarity))
    }

    /// Creates a strict matching constraint.
    #[inline]
    pub fn equals<V>(value: V) -> Self
    where
        V: AsRef<str>,
    {
        Self::Equals(EqualsTextFilter::new(value))
    }
}

impl AsRef<Box<str>> for TextFilter {
    fn as_ref(&self) -> &Box<str> {
        match self {
            TextFilter::StartsWith(starts_with_text_filter) => starts_with_text_filter.as_ref(),
            TextFilter::Matches(matches_text_filter) => matches_text_filter.as_ref(),
            TextFilter::Equals(equals_text_filter) => equals_text_filter.as_ref(),
        }
    }
}
impl AsMut<Box<str>> for TextFilter {
    fn as_mut(&mut self) -> &mut Box<str> {
        match self {
            TextFilter::StartsWith(starts_with_text_filter) => starts_with_text_filter.as_mut(),
            TextFilter::Matches(matches_text_filter) => matches_text_filter.as_mut(),
            TextFilter::Equals(equals_text_filter) => equals_text_filter.as_mut(),
        }
    }
}
