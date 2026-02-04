//! Sorted vec convenience
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Bound, Deref, RangeBounds};

use serde::{Deserialize, Serialize};
use tracing::trace;

/// A strategy for retrieving sorting key
pub trait SortKey<T> {
    /// Key type for T
    type Key: Ord + std::fmt::Debug;
    /// Get a key for T
    fn key(value: &T) -> Self::Key;
}

impl<T> SortKey<T> for ()
where
    T: Ord + Copy + Debug,
{
    type Key = T;
    fn key(value: &T) -> Self::Key {
        *value
    }
}

/// Sort by first item in a tuple
#[derive(Debug, Hash)]
pub struct TupleSort;
impl<K: Debug + Ord + Clone, V> SortKey<(K, V)> for TupleSort {
    type Key = K;

    fn key(value: &(K, V)) -> Self::Key {
        value.0.clone()
    }
}

/// A vec that is always sorted
#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[serde(transparent)]
pub struct SortedVec<T, S = ()> {
    inner: Vec<T>,
    #[serde(skip)]
    sort: PhantomData<S>,
}

impl<T, S> SortedVec<T, S>
where
    S: SortKey<T>,
    T: 'static,
{
    /// Remove items failing predicate `keep`
    /// The vec is then resorted to reflect modifications
    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let mut remove = vec![];
        for (i, v) in self.inner.iter_mut().enumerate() {
            if !keep(v) {
                remove.push(i);
            }
        }
        for r in remove.into_iter().rev() {
            self.inner.remove(r);
        }
        self.inner.sort_by_key(S::key);
    }

    /// Remove a single value from the vec
    pub fn remove<Q>(&mut self, key: &Q) -> Option<T>
    where
        Q: Borrow<S::Key>,
    {
        match search::<T, S>(&self.inner, key.borrow()) {
            Ok(found) => Some(self.inner.remove(found)),
            Err(_) => None,
        }
    }

    /// Removes a range from the sorted vec.
    /// The sorted vec remains sorted, but the returned removed items are mixed up.
    pub fn remove_range<Q, R>(&mut self, range: R) -> impl Iterator<Item = T>
    where
        Q: Clone,
        Q: Borrow<S::Key>,
        R: RangeBounds<Q>,
    {
        let Some((first, last)) = Self::get_bounds(
            &self.inner,
            range.start_bound().cloned(),
            range.end_bound().cloned(),
        ) else {
            return Removals {
                offset: self.inner.len(),
                inner: &mut self.inner,
            };
        };

        if first > last {
            panic!("reversed range")
        }
        //   . . .
        // 0 1>2 3<4 5 6 7
        // 0 4 2>3 1<5 6 7
        // 0 4 5 3>1 2<6 7
        // 0 4 5 6 1>2 3<7
        // 0 4 5 6 7|2 3 1
        let count = if last == self.inner.len() {
            self.inner.len() - first
        } else {
            (last - first).saturating_add(1)
        };

        trace!(
            "removing {count} ({first}..={last}) from {}",
            self.inner.len()
        );

        for idx in last.saturating_add(1)..self.inner.len() {
            self.inner.swap(idx, idx - count);
        }

        Removals {
            offset: self.inner.len() - count,
            inner: &mut self.inner,
        }
    }

    /// Insert a value into the vec at appropriate place.
    /// If the same key is found, insert a duplicate.
    pub fn insert(&mut self, value: T) {
        let idx = find::<T, S>(&self.inner, &value).unwrap_or_else(|v| v);
        self.inner.insert(idx, value);
    }

    /// Insert a value into the vec at appropriate place.
    /// The pre-existing value if found per key is removed and returned.
    pub fn replace(&mut self, value: T) -> Option<T> {
        match find::<T, S>(&self.inner, &value) {
            Ok(found) => Some(std::mem::replace(&mut self.inner[found], value)),
            Err(vacant) => {
                self.inner.insert(vacant, value);
                None
            }
        }
    }

    /// Check if a value key is present
    #[allow(dead_code, reason = "exploring viability")]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        Q: Borrow<S::Key>,
    {
        search::<T, S>(&self.inner, key.borrow()).is_ok()
    }

    /// Get a value by key
    #[allow(dead_code, reason = "exploring viability")]
    pub fn get<Q>(&self, key: &Q) -> Option<&T>
    where
        Q: Borrow<S::Key>,
    {
        match search::<T, S>(&self.inner, key.borrow()) {
            Ok(found) => Some(&self.inner[found]),
            Err(_) => None,
        }
    }

    /// Get a slice of values by range of keys
    #[allow(dead_code, reason = "exploring viability")]
    pub fn range<Q, R>(&self, range: R) -> &[T]
    where
        Q: Clone + Borrow<S::Key>,
        R: RangeBounds<Q>,
    {
        let Some((first, last)) = Self::get_bounds(
            &self.inner,
            range.start_bound().cloned(),
            range.end_bound().cloned(),
        ) else {
            return &[];
        };
        &self.inner[first..=last]
    }

    fn get_bounds<Q: Borrow<S::Key>>(
        slice: &[T],
        start: Bound<Q>,
        end: Bound<Q>,
    ) -> Option<(usize, usize)> {
        let mut first = match start {
            Bound::Included(start) => search::<T, S>(slice, start.borrow()),
            Bound::Excluded(start) => {
                search::<T, S>(slice, start.borrow()).map(|v| v.saturating_add(1))
            }
            Bound::Unbounded => Ok(0),
        }
        .unwrap_or_else(|v| v);

        if let Some(first_key) = slice.get(first).map(S::key) {
            while first > 1
                && let Some(earlier_key) = slice.get(first - 1).map(S::key)
                && earlier_key.cmp(&first_key) == Ordering::Equal
            {
                // include duplicates before
                first -= 1;
            }
        }

        let tail = &slice[first..];

        let mut last = match end {
            Bound::Included(end) => search::<T, S>(tail, end.borrow()).map(Some),
            Bound::Excluded(end) => search::<T, S>(tail, end.borrow()).map(|v| v.checked_sub(1)),
            Bound::Unbounded => Ok(Some(tail.len())),
        }
        .unwrap_or_else(|v| v.checked_sub(1))?;

        if let Some(last_key) = tail.get(last).map(S::key) {
            while last < tail.len().saturating_sub(1)
                && let Some(later_key) = tail.get(last + 1).map(S::key)
                && later_key.cmp(&last_key) == Ordering::Equal
            {
                // include duplicates after
                last += 1;
            }
        }

        Some((first, first.saturating_add(last)))
    }

    /// Operate on a possibly occuppied or absent entry
    /// The content is only affected after calling [`Entry::then`].
    pub fn entry(&mut self, value: T) -> Entry<'_, T, S> {
        let entry = find::<T, S>(&self.inner, &value);
        Entry {
            svec: self,
            entry,
            value,
        }
    }
}

struct Removals<'a, T> {
    inner: &'a mut Vec<T>,
    offset: usize,
}

impl<'a, T> Iterator for Removals<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.inner.len() {
            return None;
        }
        self.inner.pop()
    }
}

fn find<T, S: SortKey<T>>(slice: &[T], value: &T) -> Result<usize, usize> {
    let key = S::key(value);
    search::<T, S>(slice, &key)
}

fn search<T, S: SortKey<T>>(slice: &[T], key: &S::Key) -> Result<usize, usize> {
    slice.binary_search_by_key(key, S::key)
}

impl<T, S> Deref for SortedVec<T, S> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, S> Default for SortedVec<T, S> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            sort: Default::default(),
        }
    }
}

impl<T: Clone, S> Clone for SortedVec<T, S> {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            sort: PhantomData,
        }
    }
}

impl<T, S: SortKey<T>> From<Vec<T>> for SortedVec<T, S> {
    fn from(mut inner: Vec<T>) -> Self {
        inner.sort_by_key(S::key);
        Self {
            inner,
            sort: PhantomData,
        }
    }
}

impl<'a, T, S: SortKey<T>> IntoIterator for &'a SortedVec<T, S> {
    type Item = &'a T;

    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

/// A placeholder of an occuppied or absent entry
pub struct Entry<'a, T, S> {
    svec: &'a mut SortedVec<T, S>,
    entry: Result<usize, usize>,
    value: T,
}

impl<'a, T, S: SortKey<T>> Entry<'a, T, S> {
    /// Get a shared ref to the entry value if occupied
    pub fn as_ref(&self) -> Option<&T> {
        match self.entry {
            Ok(offset) => Some(&self.svec.inner[offset]),
            Err(_) => None,
        }
    }
    /// Operate on existing or inserted entry
    pub fn then<F: FnOnce(&mut T)>(self, update: F) -> &'a T {
        let offset = match self.entry {
            Ok(offset) => offset,
            Err(offset) => {
                self.svec.inner.insert(offset, self.value);
                offset
            }
        };
        let value = &mut self.svec.inner[offset];
        let key_before_update = S::key(value);
        update(value);
        resort(self.svec, offset, key_before_update)
    }
    /// If not found, fall back to the next entry on condition
    pub fn or_next<F: FnOnce(&T) -> bool>(mut self, condition: F) -> Self {
        match self.entry {
            Ok(_) => {}
            Err(offset) => {
                if offset < self.svec.inner.len() && condition(&self.svec.inner[offset]) {
                    self.entry = Ok(offset)
                }
            }
        }
        self
    }
    /// If not found, fall back to the previous entry on condition
    pub fn or_previous<F: FnOnce(&T) -> bool>(mut self, condition: F) -> Self {
        match self.entry {
            Ok(_) => {}
            Err(offset) => {
                if offset > 0 && condition(&self.svec.inner[offset - 1]) {
                    self.entry = Ok(offset - 1)
                }
            }
        }
        self
    }
}

impl<T, S: SortKey<T>> FromIterator<T> for SortedVec<T, S> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let vec = Vec::from_iter(iter);
        vec.into()
    }
}

fn resort<T, S: SortKey<T>>(
    svec: &mut SortedVec<T, S>,
    offset_before_update: usize,
    key_before_update: S::Key,
) -> &T {
    let value = &mut svec.inner[offset_before_update];
    let key_after_update = S::key(value);
    let new_offset = match key_before_update.cmp(&key_after_update) {
        std::cmp::Ordering::Equal => Ok(offset_before_update),
        std::cmp::Ordering::Greater => {
            search::<T, S>(&svec.inner[..offset_before_update], &key_after_update)
        }
        std::cmp::Ordering::Less => search::<T, S>(
            &svec.inner[offset_before_update.saturating_add(1)..],
            &key_after_update,
        )
        .map(|offset| offset + offset_before_update)
        .map_err(|offset| offset + offset_before_update),
    };
    trace!(
        ?offset_before_update,
        ?key_before_update,
        ?new_offset,
        ?key_after_update
    );
    match new_offset {
        Ok(_) => &svec.inner[offset_before_update],
        Err(new_offset) => {
            if new_offset == svec.inner.len() {
                let value = svec.inner.remove(offset_before_update);
                svec.inner.push(value);
            } else {
                let mut current = offset_before_update;
                let distance = new_offset.abs_diff(offset_before_update);
                let next = if new_offset < offset_before_update {
                    |v| v - 1
                } else {
                    |v| v + 1
                };
                for _ in 0..distance {
                    let next = next(current);
                    trace!(?current, ?next);
                    svec.inner.swap(std::mem::replace(&mut current, next), next);
                }
            }
            &svec.inner[new_offset]
        }
    }
}

#[cfg(test)]
mod entry_tests {

    use test_log::test;

    use super::*;

    #[test]
    fn entry_updates() {
        let mut svec: SortedVec<u8> = vec![1, 2].into();
        svec.entry(1).then(|v| *v -= 1);
        assert_eq!(svec.inner, vec![0, 2]);
    }

    #[test]
    fn entry_update_moves_forward() {
        let mut svec: SortedVec<u8> = vec![1, 2].into();
        svec.entry(1).then(|v| *v += 2);
        assert_eq!(svec.inner, vec![2, 3]);
    }
    #[test]
    fn entry_update_moves_backward() {
        let mut svec: SortedVec<u8> = vec![1, 2].into();
        svec.entry(2).then(|v| *v -= 2);
        assert_eq!(svec.inner, vec![0, 1]);
    }

    #[test]
    fn entry_update_inserts() {
        let mut svec: SortedVec<u8> = vec![1, 2].into();
        svec.entry(0).then(|_| {});
        assert_eq!(svec.inner, vec![0, 1, 2]);
    }

    #[test]
    fn entry_update_handles_duplicates() {
        let mut svec: SortedVec<u8> = vec![1, 2].into();
        svec.entry(0).then(|v| *v += 1);
        assert_eq!(svec.inner, vec![1, 1, 2]);
    }

    #[test]
    fn entry_in_empty() {
        let mut svec: SortedVec<u8> = vec![].into();
        svec.entry(0).then(|v| *v += 1);
        assert_eq!(svec.inner, vec![1]);
    }

    #[test]
    fn entry_of_one() {
        let mut svec: SortedVec<u8> = vec![1].into();
        svec.entry(1).then(|v| *v += 1);
        assert_eq!(svec.inner, vec![2]);
    }
}

#[cfg(test)]
mod remove_range_tests {

    use test_log::test;

    use super::*;

    #[test]
    fn remove_range_middle() {
        let mut svec: SortedVec<u8> = vec![0, 1, 2, 3].into();
        let mut removed = svec.remove_range(1..=2).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![0, 3]);
        assert_eq!(removed, vec![1, 2]);
    }

    #[test]
    fn remove_range_sparse() {
        let mut svec: SortedVec<u8> = vec![0, 5, 7, 20].into();
        let mut removed = svec.remove_range(2..=12).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![0, 20]);
        assert_eq!(removed, vec![5, 7]);
    }

    #[test]
    fn remove_range_head_empty() {
        let mut svec: SortedVec<u8> = vec![20].into();
        let mut removed = svec.remove_range(2..=12).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![20]);
        assert_eq!(removed, Vec::<u8>::default());
    }

    #[test]
    fn remove_range_tail_empty() {
        let mut svec: SortedVec<u8> = vec![20].into();
        let mut removed = svec.remove_range(25..=30).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![20]);
        assert_eq!(removed, Vec::<u8>::default());
    }

    #[test]
    fn remove_range_head() {
        let mut svec: SortedVec<u8> = vec![0, 1, 2, 3, 4, 5, 6].into();
        let mut removed = svec.remove_range(0..=2).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![3, 4, 5, 6]);
        assert_eq!(removed, vec![0, 1, 2]);
    }

    #[test]
    fn remove_range_tail() {
        let mut svec: SortedVec<u8> = vec![0, 1, 2, 3, 4, 5, 6].into();
        let mut removed = svec.remove_range(5..=6).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![0, 1, 2, 3, 4]);
        assert_eq!(removed, vec![5, 6]);
    }

    #[test]
    fn remove_range_duplicates() {
        let mut svec: SortedVec<u8> = vec![0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3].into();
        let mut removed = svec.remove_range(1..=2).collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![0, 0, 0, 3, 3, 3]);
        assert_eq!(removed, vec![1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn remove_range_composite() {
        let mut svec: SortedVec<(usize, usize, usize)> =
            vec![(1, 10, 1), (10, 100, 10), (100, 1000, 100)].into();
        let mut removed = svec
            .remove_range((1, 0, 0)..=(1, usize::MAX, usize::MAX))
            .collect::<Vec<_>>();
        removed.sort();
        assert_eq!(svec.inner, vec![(10, 100, 10), (100, 1000, 100)]);
        assert_eq!(removed, vec![(1, 10, 1)]);
    }
}
