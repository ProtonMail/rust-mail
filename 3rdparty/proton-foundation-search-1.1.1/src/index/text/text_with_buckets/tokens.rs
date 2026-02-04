use alloc::collections::{BTreeMap, BTreeSet};
use core::ops::{Deref, DerefMut};

use super::vocabulary::TokenRef;
use crate::index::prelude::*;

pub type TokenOccurrences = BTreeSet<(AttributeIndex, ValueIndex, EntryIndex, TokenPosition)>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Tokens(BTreeMap<TokenRef, TokenOccurrences>);

impl Tokens {
    pub fn new(btree_map: BTreeMap<TokenRef, TokenOccurrences>) -> Self {
        Self(btree_map)
    }
}
impl From<BTreeMap<TokenRef, TokenOccurrences>> for Tokens {
    fn from(value: BTreeMap<TokenRef, TokenOccurrences>) -> Self {
        Self(value)
    }
}
impl From<Tokens> for BTreeMap<TokenRef, TokenOccurrences> {
    fn from(value: Tokens) -> Self {
        value.0
    }
}
impl Deref for Tokens {
    type Target = BTreeMap<TokenRef, TokenOccurrences>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Tokens {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
