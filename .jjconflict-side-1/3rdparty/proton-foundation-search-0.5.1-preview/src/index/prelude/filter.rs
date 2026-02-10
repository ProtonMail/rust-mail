//! Index filters.

use std::collections::BTreeMap;
use std::fmt::Debug;

/// A filter that can filter the Index
pub trait Filter<V>: Debug {
    /// Given a btree map, what would be the possible values?
    fn get<'a, T>(&'a self, source: &'a BTreeMap<V, T>) -> impl Iterator<Item = (&'a V, &'a T)>;
}

impl<V> Filter<V> for V
where
    V: Ord + Debug,
{
    fn get<'a, T>(&'a self, source: &'a BTreeMap<V, T>) -> impl Iterator<Item = (&'a V, &'a T)> {
        source.get(self).into_iter().map(move |t| (self, t))
    }
}
