//! Trivial value index implementation

#![warn(missing_docs)]

use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use tracing::warn;

use crate::index::prelude::*;

mod export;
mod search;
mod store;
#[cfg(test)]
mod tests;
/// WAL-based storage implementation for trivial indices
pub mod wal;

type Entries = BTreeMap<EntryIndex, BTreeSet<ValueIndex>>;
type Attributes = BTreeMap<AttributeIndex, Entries>;
type Index<V> = BTreeMap<V, Attributes>;

/// A value type that satisfied all the required bounds
pub trait IndexableValue:
    'static + Hash + Ord + Clone + Debug + Send + Sync + Debug + IntoValue
{
}
impl<V> IndexableValue for V where
    V: 'static + Hash + Ord + Clone + Debug + Send + Sync + Debug + IntoValue
{
}

/// Implemented for types that can be represented as a [`Value`]
pub trait IntoValue {
    /// Convert value into [`Value`]
    fn into_value(self) -> Value;
}

/// Container for the integer index.
///
/// The content V is stored in a structured index:
///
/// ```rust,ignore
/// BTreeMap<V,
///     BTreeMap<AttributeIndex,
///         BTreeMap<EntryIndex,
///             BTreeSet<ValueIndex>
///         >
///     >
/// >
/// ```
///
/// in order to improve the speed of filtering for a given attribute index and filter.
///
/// The solution currently works with just one blob as this may be sufficient for small indices.
///
/// Further optimizations can be drawn by loading/saving parts of the index selectively as needed.
#[derive(Debug, Default, Clone)]
pub struct Trivial<V: IndexableValue> {
    /// Expected revision and indexed data cache.
    /// Readers get the latest snapshot, writers clone this on transaction start
    reader: Arc<ArcSwapOption<(u64, Index<V>)>>,
    writer: Arc<ArcSwapOption<(u64, Index<V>)>>,
}

impl<V: IndexableValue> Trivial<V> {
    fn name() -> &'static str {
        let mut name = type_name::<V>();
        if name == type_name::<Box<str>>() {
            name = "tag"
        }
        name
    }
}
