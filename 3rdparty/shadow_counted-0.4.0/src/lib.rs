#![doc = include_str!("../README.md")]

use std::cell::Cell;

/// An iterator that counts every iteration and optionally commits the count to a parent iterator.
///
/// Note that the `ShadowCountedIter` implements methods on its own. These can only be accessed
/// while having the iterator itself available. Thus using a `for` loop is often not possible,
/// instead a `while let Some(item) = iterator.next() {...}` loop is required to iterate over
/// its elements.
#[derive(Debug, Clone)]
pub struct ShadowCountedIter<'a, I: Iterator> {
    iter: I,
    counter: ShadowCounter<'a>,
}

impl<'a, I: Iterator> ShadowCountedIter<'a, I> {
    /// Creates a new ShadowCountedIter from an iterator.
    ///
    /// # Example
    ///
    /// ```
    /// # use shadow_counted::*;
    /// let mut iter = vec![1, 2, 3].into_iter().shadow_counted();
    /// while let Some(_) = iter.next() {}
    /// assert_eq!(iter.counter(), 3);
    /// ```
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            counter: ShadowCounter::new(),
        }
    }

    /// Creates a new nested `ShadowCountedIter` with a reference to a parent iterator.
    /// Nested iterators must be either committed to propagate their count to the parent.
    ///
    /// # Arguments
    ///
    /// * `parent`    - The parent iterator to commit the count to.
    ///
    /// # Example
    ///
    /// ```
    /// # use shadow_counted::*;
    /// let mut parent_iter = vec![1, 2, 3].into_iter().shadow_counted();
    /// parent_iter.next();
    /// # assert_eq!(parent_iter.counter(), 1);
    ///
    /// let mut nested_iter = vec![4,5].into_iter().nested_shadow_counted(&mut parent_iter);
    /// assert_eq!(nested_iter.counter(), 1);
    /// while let Some(_) = nested_iter.next() {}
    /// assert_eq!(nested_iter.counter(), 3);
    /// // destroy the nested iter while committing the count to the parent
    /// nested_iter.commit();
    /// # assert_eq!(parent_iter.counter(), 3);
    ///
    /// while let Some(_) = parent_iter.next() {}
    /// assert_eq!(parent_iter.counter(), 5);
    /// ```
    pub fn new_nested<'b, T: Iterator>(iter: I, parent: &'a mut ShadowCountedIter<'b, T>) -> Self {
        Self {
            iter,
            counter: parent.counter.nest(),
        }
    }

    /// Commits the count of a nested iterator to the parent iterator.
    /// This destroys `self` while committing the count to the parent.
    /// Only one child can commit to a parent in case these children got cloned.
    /// Any further commit will result in an error. Committing to a non nested (top level)
    /// iterator will error too. On error the iterator is returned.
    pub fn commit(mut self) -> Result<(), Self> {
        if self.counter.commit() {
            Ok(())
        } else {
            Err(self)
        }
    }

    /// This destructures the `ShadowCountedIter` and returns the iterator it wrapped.  This
    /// is useful when the counter is no longer needed but the inner iterator is and one wants
    /// to shed the lifetime that comes with the `ShadowCountedIter`.
    pub fn into_inner_iter(self) -> I {
        self.iter
    }
}

impl<I: Iterator> ShadowCountedIter<'_, I> {
    /// Allows to adjust the counter. This is required when in recursive structures only
    /// leafs shall be counted. Takes an isize so that it can be used with negative values.
    ///
    /// # Arguments
    ///
    /// * `delta` - The delta to adjust the counter by.
    ///
    /// # Example
    ///
    /// ```
    /// # use shadow_counted::*;
    /// let mut iter = vec![1, 2, 3].into_iter().shadow_counted();
    /// iter.add(2);
    /// assert_eq!(iter.counter(), 2);
    /// iter.add(-1);
    /// assert_eq!(iter.counter(), 1);
    /// ```
    #[inline]
    pub fn add(&mut self, delta: isize) {
        self.counter.add(delta)
    }

    /// Returns the iterators current count.
    ///
    /// # Example
    ///
    /// ```
    /// # use shadow_counted::*;
    /// let vec = vec![1, 2, 3];
    /// let mut iter = vec.into_iter().shadow_counted();
    /// while let Some(_) = iter.next() {}
    /// assert_eq!(iter.counter(), 3);
    /// ```
    #[inline]
    pub fn counter(&self) -> usize {
        self.counter.get()
    }
}

impl<I: Iterator> Iterator for ShadowCountedIter<'_, I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(item) => {
                *self.counter.counter.get_mut() += 1;
                Some(item)
            }
            None => None,
        }
    }
}

/// A extension trait to convert any iterator into a ShadowCountedIter.
/// This must be in scope to use the `shadow_counted` method on an iterator.
/// When possible the `From` and `Into` may be more convenient to use.
///
/// # Example
///
/// ```
/// # use shadow_counted::*;
/// let vec = vec![1, 2, 3];
/// let mut iter = vec.into_iter().shadow_counted();
/// while let Some(_) = iter.next() {}
/// assert_eq!(iter.counter(), 3);
/// ```
pub trait IntoShadowCounted {
    /// Converts the iterator into a ShadowCountedIter.
    fn shadow_counted<'a>(self) -> ShadowCountedIter<'a, Self>
    where
        Self: Sized + Iterator,
    {
        ShadowCountedIter::new(self)
    }

    /// Converts the iterator into a nested ShadowCountedIter.
    ///
    /// # Arguments
    ///
    /// * `parent` - The parent iterator to commit the count to.
    fn nested_shadow_counted<'a, I: Iterator>(
        self,
        parent: &'a mut ShadowCountedIter<'_, I>,
    ) -> ShadowCountedIter<'a, Self>
    where
        Self: Sized + Iterator,
    {
        ShadowCountedIter::new_nested(self, parent)
    }
}

impl<T: Iterator> IntoShadowCounted for T {}

/// When types can be inferred then `From` and `Into` can be used to convert an iterator into a
/// `ShadowCountedIter`.
impl<T: Iterator> From<T> for ShadowCountedIter<'_, T> {
    fn from(iter: T) -> Self {
        ShadowCountedIter::new(iter)
    }
}

/// A shadow counter that can commit its count to a parent counter.
#[derive(Debug, Clone)]
struct ShadowCounter<'a> {
    /// holds the current count including counts from children who committed here
    counter: Cell<usize>,
    /// the parents counter when this child was created
    created_at: usize,
    /// parent for nested counters
    parent: Option<&'a ShadowCounter<'a>>,
}

impl<'a> ShadowCounter<'a> {
    fn nest(&'a self) -> Self {
        Self {
            counter: self.counter.clone(),
            created_at: self.counter.get(),
            parent: Some(self),
        }
    }
}

impl ShadowCounter<'_> {
    fn new() -> Self {
        Self {
            counter: 0.into(),
            created_at: 0,
            parent: None,
        }
    }

    #[inline]
    fn get(&self) -> usize {
        self.counter.get()
    }

    fn commit(&mut self) -> bool {
        if let Some(p) = self.parent {
            if p.counter.get() == self.created_at {
                p.counter.set(self.counter.get());
                return true;
            }
        }
        false
    }

    #[inline]
    fn add(&mut self, delta: isize) {
        self.counter
            .set(self.counter.get().wrapping_add(delta as usize));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let vec: Vec<i32> = vec![];
        let mut iter = ShadowCountedIter::new(vec.into_iter());
        while iter.next().is_some() {}
        assert_eq!(iter.counter(), 0);
    }

    #[test]
    fn commit_nonnested() {
        let vec: Vec<i32> = vec![];
        let mut iter = ShadowCountedIter::new(vec.into_iter());
        while iter.next().is_some() {}
        assert!(iter.commit().is_err());
    }

    #[test]
    fn test_basic_counting() {
        let vec = vec![1, 2, 3];
        let mut iter = ShadowCountedIter::new(vec.into_iter());
        while iter.next().is_some() {}
        assert_eq!(iter.counter(), 3);
    }

    #[test]
    fn test_empty_iterator() {
        let vec: Vec<i32> = vec![];
        let mut iter = ShadowCountedIter::new(vec.into_iter());
        while iter.next().is_some() {}
        assert_eq!(iter.counter(), 0);
    }

    #[test]
    fn test_commit() {
        let mut parent_iter = vec![1, 2, 3].into_iter().shadow_counted();
        parent_iter.next();
        let mut nested_iter = vec![4, 5]
            .into_iter()
            .nested_shadow_counted(&mut parent_iter);
        nested_iter.next();
        nested_iter.commit().unwrap();
        assert_eq!(parent_iter.counter(), 2);
    }

    #[test]
    fn test_commit_twice() {
        let mut parent_iter = vec![1, 2, 3].into_iter().shadow_counted();
        parent_iter.next();
        let mut nested_iter = vec![4, 5]
            .into_iter()
            .nested_shadow_counted(&mut parent_iter);
        let nested_iter2 = nested_iter.clone();
        nested_iter.next();
        nested_iter.commit().unwrap();
        assert!(nested_iter2.commit().is_err());
        assert_eq!(parent_iter.counter(), 2);
    }

    #[test]
    fn test_two_commits() {
        let mut parent_iter = vec![1, 2, 3].into_iter().shadow_counted();
        parent_iter.next();
        let mut nested_iter = vec![4].into_iter().nested_shadow_counted(&mut parent_iter);
        nested_iter.next();
        nested_iter.commit().unwrap();
        parent_iter.next();

        // next nested iter
        let mut nested_iter = vec![5].into_iter().nested_shadow_counted(&mut parent_iter);
        nested_iter.next();
        nested_iter.commit().unwrap();

        parent_iter.next();
        assert_eq!(parent_iter.counter(), 5);
    }

    #[test]
    fn test_shadow_count_iter() {
        #[derive(Debug, PartialEq)]
        enum Nodes<'a, T> {
            Leaf(T),
            Nested(&'a [Nodes<'a, T>]),
        }

        let items = &[
            Nodes::Leaf(1),
            Nodes::Leaf(2),
            Nodes::Nested(&[Nodes::Leaf(3), Nodes::Leaf(4), Nodes::Leaf(5)]),
            Nodes::Leaf(6),
        ];

        let mut sc_iter = items.iter().shadow_counted();

        assert!((sc_iter.counter()) == (0));
        assert!((sc_iter.next()) == (Some(&Nodes::Leaf(1))));
        assert!((sc_iter.counter()) == (1));
        assert!((sc_iter.next()) == (Some(&Nodes::Leaf(2))));
        assert!((sc_iter.counter()) == (2));

        let nested = sc_iter.next().unwrap();
        assert!((sc_iter.counter()) == (3));
        assert!((nested) == (&Nodes::Nested(&[Nodes::Leaf(3), Nodes::Leaf(4), Nodes::Leaf(5)])));
        let Nodes::Nested(nested) = nested else {
            panic!()
        };
        let mut nested_iter = nested.iter().nested_shadow_counted(&mut sc_iter);
        assert_eq!(nested_iter.counter(), 3);
        assert_eq!(nested_iter.next(), Some(&Nodes::Leaf(3)));
        assert_eq!(nested_iter.counter(), 4);
        assert_eq!(nested_iter.next(), Some(&Nodes::Leaf(4)));
        assert_eq!(nested_iter.counter(), 5);
        assert_eq!(nested_iter.next(), Some(&Nodes::Leaf(5)));
        assert_eq!(nested_iter.counter(), 6);
        assert_eq!(nested_iter.next(), None);
        assert_eq!(nested_iter.counter(), 6);
        nested_iter.commit().unwrap();
        assert_eq!(sc_iter.counter(), 6);
        assert_eq!(sc_iter.next(), Some(&Nodes::Leaf(6)));
        assert_eq!(sc_iter.counter(), 7);
    }
}
