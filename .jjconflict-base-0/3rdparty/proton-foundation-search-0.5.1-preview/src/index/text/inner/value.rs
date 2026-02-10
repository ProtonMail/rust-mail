//! Indexed values

use std::ops::SubAssign;

/// The position of a trigram within a token - counted by bytes.
#[derive(
    Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TrigramPosition(pub u8);
impl TrigramPosition {
    /// Creates a new TrigramPosition.
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the offset as usize.
    pub(crate) fn offset(&self) -> usize {
        self.0 as usize
    }
}
impl From<u8> for TrigramPosition {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

/// Reference to the offset of an indexed occurrence
#[derive(
    Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct OccurrenceRef(pub u32);
impl OccurrenceRef {
    /// Creates a new OccurrenceRef.
    pub fn new(value: u32) -> Self {
        Self(value)
    }
    /// Returns the offset as usize.
    pub fn offset(&self) -> usize {
        self.0 as usize
    }
}
impl From<usize> for OccurrenceRef {
    fn from(value: usize) -> Self {
        #[allow(clippy::expect_used)]
        let value = u32::try_from(value).expect(
            "To remain compatible with 32bit arch, we cannot index over u32 even on 64bits",
        );
        Self(value)
    }
}
impl SubAssign<usize> for OccurrenceRef {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 = ((self.0 as usize) - rhs) as u32;
    }
}

/// Reference to the offset in `tokens` to a unique indexed token
#[derive(
    Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TokenRef(pub u32);

impl TokenRef {
    /// Creates a new TokenRef.
    pub fn new(value: u32) -> Self {
        Self(value)
    }
    /// Returns the offset as usize.
    pub fn offset(&self) -> usize {
        self.0 as usize
    }
}
impl From<usize> for TokenRef {
    fn from(value: usize) -> Self {
        #[allow(clippy::expect_used)]
        let value = u32::try_from(value).expect(
            "To remain compatible with 32bit arch, we cannot index over u32 even on 64bits",
        );
        Self(value)
    }
}
impl SubAssign<usize> for TokenRef {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 = ((self.0 as usize) - rhs) as u32;
    }
}
