//! Custom traits for mail functionality.

/// Custom equality trait that allows selective field comparison.
///
/// This trait is similar to `Eq` but provides the ability to skip certain fields
/// from equality comparison using the `#[scroller_eq(skip)]` attribute when used
/// with the derive macro.
///
/// This is particularly useful for conversation objects where certain metadata
/// fields (like timestamps, counts, or UI state) shouldn't affect equality
/// for scrolling/comparison purposes.
///
/// # Example
///
/// ```rust
/// use mail_common_derive::ScrollerEq;
/// use proton_mail_common::traits::ScrollerEq as ScrollerEqTrait;
///
/// #[derive(ScrollerEq)]
/// struct Conversation {
///     id: u64,
///     subject: String,
///     #[scroller_eq(skip)]
///     last_updated: u64,  // This field will be ignored in equality comparison
///     #[scroller_eq(skip)]
///     unread_count: u32,  // This field will be ignored in equality comparison
/// }
///
/// let conv1 = Conversation {
///     id: 1,
///     subject: "Hello".to_string(),
///     last_updated: 100,
///     unread_count: 5,
/// };
///
/// let conv2 = Conversation {
///     id: 1,
///     subject: "Hello".to_string(),
///     last_updated: 200,  // Different timestamp
///     unread_count: 3,    // Different count
/// };
///
/// assert!(conv1.scroller_eq(&conv2)); // Returns true despite different skipped fields
/// ```
use std::fmt::Debug;

pub trait ScrollerEq: Debug {
    /// Checks if two instances are equal, ignoring fields marked with `#[scroller_eq(skip)]`.
    fn scroller_eq(&self, other: &Self) -> bool;
}

#[cfg(test)]
mod tests;

impl<T: ScrollerEq> ScrollerEq for Vec<T> {
    fn scroller_eq(&self, other: &Self) -> bool {
        (self.as_slice()).scroller_eq(other.as_slice())
    }
}

impl<T: ScrollerEq> ScrollerEq for [T] {
    #[allow(clippy::print_stdout)]
    fn scroller_eq(&self, other: &Self) -> bool {
        let equal = self.len() == other.len()
            && self.iter().zip(other.iter()).all(|(a, b)| a.scroller_eq(b));

        #[cfg(test)]
        {
            if !equal {
                println!("Not equal: \n{self:?}\n{other:?}",);
            }
        }

        equal
    }
}
