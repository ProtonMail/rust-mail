use core::ops::RangeBounds;

use serde::{Deserialize, Serialize};

use super::vocabulary::TokenRef;
use crate::index::text::trigram::{Trigram, TrigramPosition, Trigrams as _};
use crate::svec::{SortedVec, TupleSort};

#[derive(Debug, Default, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrigramIndex {
    trigrams: SortedVec<((Trigram, u8), SortedVec<TokenRef>), TupleSort>,
}

impl Clone for TrigramIndex {
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            trigrams: self.trigrams.clone(),
        }
    }
}

impl TrigramIndex {
    pub fn get<R, T, I>(
        &self,
        trigrams: I,
    ) -> impl Iterator<Item = (TrigramPosition, &Trigram, TokenRef)>
    where
        R: Clone + RangeBounds<u8>,
        T: AsRef<str>,
        I: Iterator<Item = (R, T)>,
    {
        trigrams.flat_map(|(range, trigram)| {
            let trigram: Trigram = trigram.into();
            let start = range.start_bound().map(|pos| (trigram, *pos));
            let end = range.end_bound().map(|pos| (trigram, *pos));
            self.trigrams
                .range((start, end))
                .iter()
                .flat_map(move |((trigram, pos), tokens)| {
                    tokens
                        .iter()
                        .map(move |token_ref| (TrigramPosition(*pos), trigram, *token_ref))
                })
        })
    }

    pub fn insert(&mut self, token_ref: TokenRef, token: &str) {
        for (pos, trigram) in token.trigrams() {
            self.trigrams
                .entry(((trigram.into(), pos as u8), Default::default()))
                .then(|(_, tokens)| {
                    tokens.replace(token_ref);
                });
        }
    }

    pub fn remove(&mut self, token_ref: &TokenRef) {
        self.trigrams.retain(|(_, v)| {
            v.remove(&token_ref);
            !v.is_empty()
        });
    }
}

#[cfg(test)]
impl TrigramIndex {
    pub fn len(&self) -> usize {
        use itertools::Itertools;
        self.trigrams.iter().map(|((t, _), _)| t).dedup().count()
    }
}
