//! ## Traits
//!
//! This module defines core traits used throughout the `muon` crate.

/// A type that can be converted into a dynamic type.
pub trait IntoDyn<T> {
    /// Convert `self` into a dynamic type.
    fn into_dyn(self) -> T;
}
