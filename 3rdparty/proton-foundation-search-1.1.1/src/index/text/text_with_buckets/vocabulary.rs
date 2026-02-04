use core::ops::RangeBounds;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use tracing::trace;

use super::trigrams::TrigramIndex;
use crate::index::text::token::Token;
use crate::index::text::trigram::{Trigram, TrigramPosition};
use crate::svec::{SortedVec, TupleSort};

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct TokenRef(pub usize);

#[derive(Debug, Default, Hash, Serialize, Deserialize)]
pub struct Vocabulary {
    next: TokenRef,
    tokens: SortedVec<(TokenRef, Token), TupleSort>,
    #[serde(skip)]
    inverse: SortedVec<(Token, TokenRef), TupleSort>,
    trigrams: TrigramIndex,
}

impl Clone for Vocabulary {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            next: self.next,
            tokens: self.tokens.clone(),
            inverse: self.inverse.clone(),
            trigrams: self.trigrams.clone(),
        }
    }
}

impl Vocabulary {
    /// Get a token reference
    pub fn get(&self, token_ref: &TokenRef) -> Option<&Token> {
        self.tokens.get(token_ref).map(|(_, v)| v)
    }

    pub fn get_trigrams<R, T, I>(
        &self,
        trigrams: I,
    ) -> impl Iterator<Item = (TrigramPosition, &Trigram, TokenRef, &Token)>
    where
        R: Clone + RangeBounds<u8>,
        T: AsRef<str>,
        I: Iterator<Item = (R, T)>,
    {
        self.trigrams
            .get(trigrams)
            .filter_map(|(pos, trigram, token_ref)| {
                let token = self.get(&token_ref)?;
                Some((pos, trigram, token_ref, token))
            })
    }

    /// Insert a token or get ones token reference
    pub fn insert(&mut self, token: Token) -> TokenRef {
        self.populate_inverse();

        let Self {
            next,
            tokens,
            inverse,
            trigrams,
        } = self;

        let entry = inverse.entry((token.clone(), TokenRef(next.0)));

        if let Some((_, existing)) = entry.as_ref() {
            return *existing;
        }

        let (_, token_ref) = entry.then(|(_, token_ref)| {
            *next = TokenRef(next.0 + 1);
            trigrams.insert(*token_ref, &token);
            tokens.insert((*token_ref, token));
        });
        *token_ref
    }

    /// Remove a token, getting the removed token reference
    pub fn remove(&mut self, token_ref: &TokenRef) -> Option<Token> {
        let (token_ref, token) = self.tokens.remove(token_ref)?;
        trace!("removing token: {token_ref:?}: {token:?}",);
        self.trigrams.remove(&token_ref);
        self.inverse.remove(&token);
        Some(token)
    }

    fn populate_inverse(&mut self) {
        if self.inverse.is_empty() && !self.tokens.is_empty() {
            // deserialized vocabulary does not have inverse index
            self.inverse = self.tokens.iter().map(|(k, v)| (v.clone(), *k)).collect();
        }
    }
}

#[cfg(test)]
impl Vocabulary {
    pub fn count_tokens(&self) -> usize {
        self.tokens.len()
    }
    pub fn count_trigrams(&self) -> usize {
        self.trigrams.len()
    }
}
