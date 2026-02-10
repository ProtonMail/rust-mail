//! This module contains traits that are simply delegated to the underlying [`NonEmptyString`].
//!
//! The order of traits implemented here follow the same structure as defined in the standard library.
//! When adding new ones, add them in the same order, so it's easy to keep track.
//!
//! The source at the moment of writing is [here](https://github.com/rust-lang/rust/blob/22d41ae90facbffdef9115809e8b6c1f71ebbf7c/library/alloc/src/string.rs#L2019).
//! The link to master is [here](https://github.com/rust-lang/rust/blob/master/library/alloc/src/string.rs#L2019).
//!

use crate::NonEmptyString;
use std::borrow::Cow;
use std::fmt::Display;
use std::ops::{self, Add, AddAssign};

#[cfg(not(no_global_oom_handling))]
impl Extend<char> for NonEmptyString {
    fn extend<I: IntoIterator<Item = char>>(&mut self, iter: I) {
        self.0.extend(iter)
    }
}

#[cfg(not(no_global_oom_handling))]
impl<'a> Extend<&'a char> for NonEmptyString {
    fn extend<I: IntoIterator<Item = &'a char>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

#[cfg(not(no_global_oom_handling))]
impl<'a> Extend<&'a str> for NonEmptyString {
    fn extend<I: IntoIterator<Item = &'a str>>(&mut self, iter: I) {
        iter.into_iter().for_each(move |s| self.push_str(s));
    }
}

#[cfg(not(no_global_oom_handling))]
impl Extend<Box<str>> for NonEmptyString {
    fn extend<I: IntoIterator<Item = Box<str>>>(&mut self, iter: I) {
        iter.into_iter().for_each(move |s| self.push_str(&s));
    }
}

#[cfg(not(no_global_oom_handling))]
impl Extend<String> for NonEmptyString {
    fn extend<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        iter.into_iter().for_each(move |s| self.0.push_str(&s));
    }
}

#[cfg(not(no_global_oom_handling))]
impl<'a> Extend<Cow<'a, str>> for NonEmptyString {
    fn extend<I: IntoIterator<Item = Cow<'a, str>>>(&mut self, iter: I) {
        iter.into_iter().for_each(move |s| self.push_str(&s));
    }
}

macro_rules! impl_eq {
    ($lhs:ty, $rhs: ty) => {
        #[allow(unused_lifetimes)]
        impl<'a, 'b> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
        }

        #[allow(unused_lifetimes)]
        impl<'a, 'b> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
        }
    };
}

impl_eq! { NonEmptyString, str }
impl_eq! { NonEmptyString, &'a str }
#[cfg(not(no_global_oom_handling))]
impl_eq! { Cow<'a, str>, NonEmptyString }
#[cfg(not(no_global_oom_handling))]
impl_eq! { String, NonEmptyString }

// No sensible implementation for Default
// impl Default for NonEmptyString {
// }

impl Display for NonEmptyString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

// Derived:
// impl fmt::Debug for String {
// }

// Derived:
// impl hash::Hash for NonEmptyString {
// }

#[cfg(not(no_global_oom_handling))]
impl Add<&str> for NonEmptyString {
    type Output = NonEmptyString;

    #[inline]
    fn add(mut self, other: &str) -> NonEmptyString {
        self.push_str(other);
        self
    }
}

#[cfg(not(no_global_oom_handling))]
impl AddAssign<&str> for NonEmptyString {
    #[inline]
    fn add_assign(&mut self, other: &str) {
        self.push_str(other);
    }
}

macro_rules! index_impl {
    ($t:ty) => {
        impl ops::Index<$t> for NonEmptyString {
            type Output = str;

            #[inline]
            fn index(&self, index: $t) -> &str {
                <String as ops::Index<$t>>::index(&self.0, index)
            }
        }
    };
}

index_impl!(ops::Range<usize>);
index_impl!(ops::RangeTo<usize>);
index_impl!(ops::RangeFrom<usize>);
index_impl!(ops::RangeFull);
index_impl!(ops::RangeInclusive<usize>);
index_impl!(ops::RangeToInclusive<usize>);

// Not 100% sure if index_mut allows turning a NonEmptyString into an empty string or not, let's leave it out until sure.
// macro_rules! index_mut_impl {
//     ($t:ty) => {
//         impl ops::IndexMut<$t> for NonEmptyString {
//             #[inline]
//             fn index_mut(&mut self, index: $t) -> &mut str {
//                 <String as ops::IndexMut<$t>>::index_mut(&mut self.0, index)
//             }
//         }
//     };
// }

// index_mut_impl!(ops::Range<usize>);
// index_mut_impl!(ops::RangeTo<usize>);
// index_mut_impl!(ops::RangeFrom<usize>);
// index_mut_impl!(ops::RangeFull);
// index_mut_impl!(ops::RangeInclusive<usize>);
// index_mut_impl!(ops::RangeToInclusive<usize>);

// Point of discussion, see https://github.com/MidasLamb/non-empty-string/pull/11
// impl ops::Deref for NonEmptyString {
// }

// This would mean people could empty out the string, so no.
// impl ops::DerefMut for NonEmptyString {
// }

#[cfg(not(no_global_oom_handling))]
impl std::fmt::Write for NonEmptyString {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.push_str(s);
        Ok(())
    }

    #[inline]
    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.push(c);
        Ok(())
    }
}
