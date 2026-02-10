//! Shared index implementation code

#![warn(missing_docs)]

use std::num::TryFromIntError;

mod dump;
mod filter;
mod search;
mod store;
pub mod wal;

pub use self::dump::*;
pub use self::filter::*;
pub use self::search::*;
pub use self::store::*;

/// Index implementation supertrait
pub trait Index: IndexStore + IndexSearch + IndexExport + std::fmt::Debug + Send + Sync {}
impl<T: IndexStore + IndexSearch + IndexExport + std::fmt::Debug + Send + Sync> Index for T {}

/// Representation of an attribute in a document.
#[derive(
    Clone,
    Copy,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Display,
    serde::Serialize,
    serde::Deserialize,
)]
#[display("Attribute[{}]", self.0)]
#[serde(transparent)]
pub struct AttributeIndex(pub u8);
impl TryFrom<usize> for AttributeIndex {
    type Error = TryFromIntError;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u8::try_from(value).map(AttributeIndex)
    }
}
impl From<u8> for AttributeIndex {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

/// Index of an entry in the collection.
#[derive(
    Clone,
    Copy,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Display,
    serde::Serialize,
    serde::Deserialize,
)]
#[display("Entry[{}]", self.0)]
#[serde(transparent)]
pub struct EntryIndex(pub u32);
impl TryFrom<usize> for EntryIndex {
    type Error = TryFromIntError;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u32::try_from(value).map(EntryIndex)
    }
}
impl From<u32> for EntryIndex {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Index of the value in the attributes for an entry
#[derive(
    Clone,
    Copy,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Display,
    serde::Serialize,
    serde::Deserialize,
)]
#[display("AttributeValue[{}]", self.0)]
#[serde(transparent)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct ValueIndex(pub usize);

impl From<usize> for ValueIndex {
    fn from(value: usize) -> Self {
        ValueIndex(value)
    }
}

/// Sequential position of the token in the text blob value.
#[derive(
    Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct TokenPosition(pub usize);
/// Creates a new TokenPosition.
impl From<usize> for TokenPosition {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

#[test]
fn test_can_be_trait_object() {
    let _: Option<Box<dyn Index>> = None;
}
