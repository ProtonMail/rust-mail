//! Query options for search results

use std::ops::{Deref, DerefMut};

use crate::query::option::{QueryOption, QueryOptions};

const DEFAULT_COLLECT_STATS: bool = true;
const DEFAULT_COLLECT_POSITIONS: bool = true;

/// Should search statistics be collected? Default: true.
///
/// Example: Opt out from stats collection.
///
/// ```rust
/// use proton_foundation_search::query::option::QueryOptions;
/// use proton_foundation_search::query::option::results::CollectStats;
/// let opt = QueryOptions::default().with::<CollectStats>(|value| **value = false);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CollectStats(bool);
impl CollectStats {
    /// Get the option value
    pub fn get(options: &QueryOptions) -> bool {
        *options.get::<Self>().copied().unwrap_or_default()
    }
}
impl QueryOption for CollectStats {}
impl Default for CollectStats {
    fn default() -> Self {
        DEFAULT_COLLECT_STATS.into()
    }
}
impl Deref for CollectStats {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for CollectStats {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<bool> for CollectStats {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
impl From<CollectStats> for bool {
    fn from(value: CollectStats) -> Self {
        value.0
    }
}

/// Should matched token positions be collected during search? Default: true.
///
/// Example: Opt out from positions collection.
///
/// ```rust
/// use proton_foundation_search::query::option::QueryOptions;
/// use proton_foundation_search::query::option::results::CollectPositions;
/// let opt = QueryOptions::default().with::<CollectPositions>(|value| **value = false);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CollectPositions(bool);
impl CollectPositions {
    /// Get the option value
    pub fn get(options: &QueryOptions) -> bool {
        *options.get::<Self>().copied().unwrap_or_default()
    }
}
impl QueryOption for CollectPositions {}
impl Default for CollectPositions {
    fn default() -> Self {
        DEFAULT_COLLECT_POSITIONS.into()
    }
}
impl Deref for CollectPositions {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for CollectPositions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<bool> for CollectPositions {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
impl From<CollectPositions> for bool {
    fn from(value: CollectPositions) -> Self {
        value.0
    }
}
