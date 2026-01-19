//! The crate's prelude.

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Trait that checks if the container is empty.
pub trait IsEmpty {
    /// Returns `true`, if empty, otherwise `false`.
    fn is_empty(&self) -> bool;
}

impl<K, V> IsEmpty for BTreeMap<K, V> {
    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

impl<K, V> IsEmpty for HashMap<K, V> {
    fn is_empty(&self) -> bool {
        HashMap::is_empty(self)
    }
}

impl<K, V> IsEmpty for HashSet<K, V> {
    fn is_empty(&self) -> bool {
        HashSet::is_empty(self)
    }
}

/// Trait that provides mutable access to a key's associated value.
pub trait WithMut<K, V> {
    /// Returns the mutable value in the callback if it exists and, if the value
    /// is empty after calling the callback, it's removed from self.
    fn with_mut<Q, Callback>(&mut self, key: &Q, callback: Callback) -> bool
    where
        Callback: FnOnce(&mut V) -> bool,
        K: Borrow<Q>,
        Q: ?Sized,
        Q: std::cmp::Ord,
        Q: std::hash::Hash;

    /// Iterates through all the items, calls the callback and if the value is empty
    /// after calling the callback, it's removed from self.
    fn iter_with_mut<Callback>(&mut self, callback: Callback) -> bool
    where
        Callback: FnMut((&K, &mut V)) -> bool;
}

impl<K, V> WithMut<K, V> for BTreeMap<K, V>
where
    K: Clone,
    K: std::cmp::Eq,
    K: std::cmp::Ord,
    K: std::hash::Hash,
    V: Default,
    V: IsEmpty,
{
    #[tracing::instrument(skip_all)]
    fn with_mut<Q, Callback>(&mut self, key: &Q, callback: Callback) -> bool
    where
        Callback: FnOnce(&mut V) -> bool,
        K: Borrow<Q>,
        Q: ?Sized,
        Q: std::cmp::Ord,
        Q: std::hash::Hash,
    {
        let Some(value) = self.get_mut(key) else {
            return false;
        };

        let changed = callback(value);

        if value.is_empty() {
            self.remove(key);
        }

        changed
    }

    #[tracing::instrument(skip_all)]
    fn iter_with_mut<Callback>(&mut self, mut callback: Callback) -> bool
    where
        Callback: FnMut((&K, &mut V)) -> bool,
    {
        let mut keys_to_remove = Vec::new();

        let changed = self.iter_mut().fold(false, |res, (key, value)| {
            let changed = callback((key, value));

            if value.is_empty() {
                keys_to_remove.push(key.clone());
            }

            res || changed
        });

        for key in keys_to_remove {
            self.remove(&key);
        }

        changed
    }
}

impl<K, V> WithMut<K, V> for HashMap<K, V>
where
    K: Clone,
    K: std::cmp::Eq,
    K: std::hash::Hash,
    V: Default,
    V: IsEmpty,
{
    #[tracing::instrument(skip_all)]
    fn with_mut<Q, Callback>(&mut self, key: &Q, callback: Callback) -> bool
    where
        Callback: FnOnce(&mut V) -> bool,
        K: Borrow<Q>,
        Q: ?Sized,
        Q: std::cmp::Ord,
        Q: std::hash::Hash,
    {
        let Some(value) = self.get_mut(key) else {
            return false;
        };

        let changed = callback(value);

        if value.is_empty() {
            self.remove(key);
        }

        changed
    }

    #[tracing::instrument(skip_all)]
    fn iter_with_mut<Callback>(&mut self, mut callback: Callback) -> bool
    where
        Callback: FnMut((&K, &mut V)) -> bool,
    {
        let mut keys_to_remove = Vec::new();

        let changed = self.iter_mut().fold(false, |res, (key, value)| {
            let changed = callback((key, value));

            if value.is_empty() {
                keys_to_remove.push(key.clone());
            }

            res || changed
        });

        for key in keys_to_remove {
            self.remove(&key);
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use test_log::test;

    use super::*;

    #[test]
    fn check_is_empty() {
        let mut entries = HashSet::new();
        assert!(IsEmpty::is_empty(&entries));
        entries.insert(42);
        assert!(!IsEmpty::is_empty(&entries));
    }

    macro_rules! check_when_mut_trait {
        ($name:ident) => {
            #[test_log::test]
            fn check_if_empty() {
                let mut entries = $name::new();
                assert!(IsEmpty::is_empty(&entries));
                entries.insert(42, 42);
                assert!(!IsEmpty::is_empty(&entries));
            }

            #[test_log::test]
            fn should_remove_empty_children() {
                let mut posting: $name<u8, $name<u8, u8>> = $name::default();
                assert!(!posting.with_mut(&42, |_| false));

                posting.insert(10, {
                    let mut inner = $name::default();
                    inner.insert(20, 30);
                    inner.insert(40, 50);
                    inner
                });

                posting.insert(60, {
                    let mut inner = $name::default();
                    inner.insert(70, 80);
                    inner
                });

                assert!(posting.with_mut(&60, |inner| { inner.remove(&70).is_some() }));
                assert!(!posting.contains_key(&60));

                posting.insert(60, $name::default());
                assert!(!posting.with_mut(&60, |_| { false }));
                assert!(!posting.contains_key(&60));

                assert!(!posting.with_mut(&15, |_| { true }));
                assert!(!posting.contains_key(&60));
            }
        };
    }

    mod hash_map {
        use super::*;

        check_when_mut_trait!(HashMap);
    }

    mod btree_map {
        use super::*;

        check_when_mut_trait!(BTreeMap);
    }
}
