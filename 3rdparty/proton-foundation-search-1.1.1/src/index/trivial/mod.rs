//! Trivial value index implementation

#![warn(missing_docs)]

use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::document::Value;
use crate::index::prelude::*;

mod export;
mod search;
mod store;
#[cfg(test)]
mod tests;

type Entries = BTreeMap<EntryIndex, BTreeSet<ValueIndex>>;
type Attributes = BTreeMap<AttributeIndex, Entries>;
type Index<V> = BTreeMap<V, Attributes>;

/// A value type that satisfied all the required bounds
pub trait IndexableValue:
    'static
    + Hash
    + Ord
    + Clone
    + Debug
    + Send
    + Sync
    + Debug
    + Into<Value>
    + Serialize
    + for<'de> Deserialize<'de>
{
}
impl<V> IndexableValue for V where
    V: 'static
        + Hash
        + Ord
        + Clone
        + Debug
        + Send
        + Sync
        + Debug
        + Into<Value>
        + for<'de> Deserialize<'de>
        + Serialize
{
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
    value: PhantomData<V>,
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
