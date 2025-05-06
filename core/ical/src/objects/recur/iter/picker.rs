use std::collections::{BTreeSet, VecDeque};

/// Filters items by their indices, used to implement the BYSETPOS rule.
///
/// # Abstract
///
/// BYSETPOS allows to choose specific recurrence(s) from a larger recurrence
/// set - e.g. `FREQ=MONTHLY;BYDAY=MO,TU` emits - for each month - all of its
/// Mondays and Tuesdays, and slapping `BYSETPOS=-1` limits the recurrence to
/// just the *last* Monday or Tuesday of each month.
///
/// (`BYSETPOS=1,-1` would choose the first and the last Monday or Tuesday etc.)
///
/// Abstract version of this problem boils down to implementing this:
///
/// ```
/// fn pick<T>(
///     idxs: &[i16],
///     iterator: impl Iterator<Item = T>,
/// ) -> impl Iterator<Item = T>
/// where
///     T: Ord,
/// {
///     [].into_iter()
/// }
/// ```
///
/// ... expecting:
///
/// - `pick([1], [100, 200, 300]) = [100]`
/// - `pick([-1], [100, 200, 300]) = [300]`
/// - `pick([1, 3], [100, 200, 300]) = [100, 300]`
/// - `pick([1, -3], [100, 200, 300]) = [100]`
/// - etc.
///
/// We must also preserve monotonicity, so:
///
/// ```text
/// pick([2, -3], [100, 200, 300]) = [100, 200]
///                                   #-3  #2
/// ```
///
/// ... but:
///
/// ```text
/// pick([2, -3], [100, 200, 300, 400, 500]) = [200, 300]
///                                             #2   #-3
/// ```
///
/// # Implementation
///
/// The simplest approach would be to collect the inner-iterator, normalize
/// negative indices via `collected_iterator.len()` and then emit matching items
/// from `collected_iterator` -- but we can get smarter!
///
/// a. Given only-positive indices, we don't have to collect anything - we can
///    keep track of the current item's index and use it to filter items as they
///    come.
///
///    E.g. `idxs = [2, 5]` basically means "skip one, emit one, skip three,
///    emit one, ignore rest".
///
/// b. Given only-negative indices, we have to collect only as much items as
///    the most-extreme index indicates.
///
///    E.g. `idxs = [-3, -1]` requires allocating memory only for three items,
///    and `VecDeque` is ideal for keeping track of them.
///
/// c. Given both positive and negative items, we have to collect the matching
///    front items (kinda as in A) and the matching back items (as in B), and
///    then do an ordered-zip on both collections.
///
/// See tests for reference.
#[derive(Clone, Debug)]
pub struct Picker<T> {
    front: VecDeque<T>,
    front_idxs: BTreeSet<i16>,
    back: VecDeque<(i16, T)>,
    back_idxs: BTreeSet<i16>,
    state: PickerState,
}

impl<T> Picker<T> {
    pub fn new(idxs: impl IntoIterator<Item = i16>) -> Option<Self> {
        let mut front_cap = 0;
        let mut front_idxs = BTreeSet::new();
        let mut back_cap = 0;
        let mut back_idxs = BTreeSet::new();

        for idx in idxs {
            if idx > 0 {
                front_cap += 1;
                front_idxs.insert(idx);
            } else {
                back_cap = back_cap.max(usize::from(idx.unsigned_abs()));
                back_idxs.insert(idx);
            }
        }

        if front_cap == 0 && back_cap == 0 {
            None
        } else {
            let front = if back_cap > 0 {
                VecDeque::with_capacity(front_cap)
            } else {
                VecDeque::new()
            };

            let back = VecDeque::with_capacity(back_cap);

            Some(Self {
                front,
                front_idxs,
                back,
                back_idxs,
                state: PickerState::Opened { len: 0 },
            })
        }
    }

    /// Adds an item into the collection.
    ///
    /// This function returns the item back - i.e. it returns `Some(item)` - if
    /// the item already matches the predicate and thus can be safely emitted by
    /// the caller.
    ///
    /// See tests for reference, but the general idea is that `push()` tries to
    /// be lazy and returns `Some(item)` as often as possible - only when items
    /// have to be collected or given item doesn't match the predicate¹ this
    /// function returns `None`.
    ///
    /// ¹ e.g. there's `BYSETPOS=2`, but we're witnessing the first item
    pub fn push(&mut self, item: T) -> Option<T> {
        let PickerState::Opened { len } = &mut self.state else {
            #[cfg(debug_assertions)]
            unreachable!();

            #[cfg(not(debug_assertions))]
            return None;
        };

        *len += 1;

        if self.front_idxs.contains(len) {
            if self.back_idxs.is_empty() {
                return Some(item);
            }

            self.front.push_back(item);

            return None;
        }

        if !self.back_idxs.is_empty() {
            if self.back.len() == self.back.capacity() {
                self.back.pop_front();
            }

            self.back.push_back((*len, item));
        }

        None
    }

    /// Closes the collection, informing us that the upper-iterator has finished
    /// working.
    ///
    /// After calling this function you're only allowed to call [`Self::pull()`]
    /// and you must do so until it returns `None`, meaning that the collection
    /// has been drained.
    ///
    /// After [`Self::pull()`] returns `None`, you can go back to invoking
    /// [`Self::push()`] .
    pub fn close(&mut self) {
        let PickerState::Opened { len } = self.state else {
            #[cfg(debug_assertions)]
            unreachable!();

            #[cfg(not(debug_assertions))]
            return;
        };

        // Since now we know the length of the upper-iterator, we can remap the
        // negative indices to positive ones.
        //
        // E.g. given `len=10`, we'd remap `idx=-2` to `idx=9`.
        //
        // Note that we use one-based indexing, for compatibility with with how
        // BYSETPOS works.
        for (idx, _) in &mut self.back {
            *idx = *idx - len - 1;
        }

        self.state = PickerState::Closed;
    }

    /// Pulls item from the collection.
    ///
    /// This function is used to handle cases B and C as described in the
    /// top-comment.
    pub fn pull(&mut self) -> Option<T>
    where
        T: Ord,
    {
        let PickerState::Closed = self.state else {
            return None;
        };

        // Select the front item.
        //
        // Since `self.front` contains only those items which already match
        // `self.front_idxs`, we don't have to have any extra filtering here.
        let front = self.front.front();

        // Select the back item.
        //
        // Since `self.back` contains _all_ of the last items, we have to apply
        // extra filtering to skip over those items which don't match
        // `self.back_idxs`.
        //
        // E.g. given `BYSETPOS=-2`, `self.back` will contain two last items,
        // out of which we need to skip the first one here.
        //
        // N.B. this could be done over [`Self::close()`], but doing it here
        //      avoids building another collection
        let back = loop {
            let Some((back_idx, back_item)) = self.back.front() else {
                break None;
            };

            if self.back_idxs.contains(back_idx) {
                break Some(back_item);
            }

            self.back.pop_front();
        };

        // Choose the earlier item.
        //
        // This operation preserves monotonicity, assuming the upper-iterator is
        // monotonic (which it should be, otherwise we're screwed anyway).
        match (front, back) {
            (Some(front), Some(back)) => {
                if front < back {
                    self.front.pop_front()
                } else {
                    self.back.pop_front().map(|(_idx, item)| item)
                }
            }

            (Some(_), None) => self.front.pop_front(),
            (None, Some(_)) => self.back.pop_front().map(|(_idx, item)| item),

            (None, None) => {
                self.state = PickerState::Opened { len: 0 };

                None
            }
        }
    }

    pub fn reset(&mut self) {
        self.front.clear();
        self.back.clear();
        self.state = PickerState::Opened { len: 0 };
    }
}

#[derive(Clone, Copy, Debug)]
enum PickerState {
    Opened { len: i16 },
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert!(Picker::<i32>::new([]).is_none());
    }

    #[test]
    fn just_front() {
        let mut target = Picker::new([1]).unwrap();

        assert_eq!(Some(10), target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([2]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(Some(20), target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([3]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(Some(30), target.push(30));

        target.close();

        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([1, 3]).unwrap();

        assert_eq!(Some(10), target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(Some(30), target.push(30));

        target.close();

        assert_eq!(None, target.pull());
    }

    #[test]
    fn just_back() {
        let mut target = Picker::new([-1]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(30), target.pull());
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-2]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(20), target.pull());
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-3]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(10), target.pull());
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-1, -3]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(10), target.pull());
        assert_eq!(Some(30), target.pull());
        assert_eq!(None, target.pull());
    }

    #[test]
    fn back_and_front() {
        let mut target = Picker::new([-1, 1]).unwrap();

        assert_eq!(None, target.push(10));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -1 and 1
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-1, 1]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -1
        assert_eq!(Some(20), target.pull()); // matches 1
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-1, 1]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -1
        assert_eq!(Some(30), target.pull()); // matches 1
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-3, 2]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -3
        assert_eq!(Some(20), target.pull()); // matches 2
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-3, 3]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -3
        assert_eq!(Some(30), target.pull()); // matches 3
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-5, -3, -1, 2, 4]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));
        assert_eq!(None, target.push(40));
        assert_eq!(None, target.push(50));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -5
        assert_eq!(Some(20), target.pull()); // matches 2
        assert_eq!(Some(30), target.pull()); // matches -3
        assert_eq!(Some(40), target.pull()); // matches 4
        assert_eq!(Some(50), target.pull()); // matches -1
        assert_eq!(None, target.pull());

        // ---

        let mut target = Picker::new([-5, 3]).unwrap();

        assert_eq!(None, target.push(10));
        assert_eq!(None, target.push(20));
        assert_eq!(None, target.push(30));
        assert_eq!(None, target.push(40));
        assert_eq!(None, target.push(50));

        target.close();

        assert_eq!(Some(10), target.pull()); // matches -5
        assert_eq!(Some(30), target.pull()); // matches 3
        assert_eq!(None, target.pull());
    }
}
