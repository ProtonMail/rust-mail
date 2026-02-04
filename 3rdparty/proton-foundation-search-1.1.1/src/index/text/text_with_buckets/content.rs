use alloc::sync::Arc;
use core::ops::ControlFlow;

use serde::{Deserialize, Serialize};

use super::tokens::Tokens;
use super::vocabulary::Vocabulary;
use crate::blob::{Blob, LoadEvent};
use crate::index::prelude::*;
use crate::index::text::text_with_buckets::vocabulary::TokenRef;
use crate::prelude::*;
use crate::svec::SortedVec;

impl Content {
    pub fn fetch_tokens(&mut self, bucket: usize) -> ControlFlow<LoadEvent, Option<Arc<Tokens>>> {
        match self
            .occurrences
            .get_mut(bucket)
            .map(|b| b.occurrences.fetch_and_free())
        {
            None => ControlFlow::Continue(None),
            Some(content) => ControlFlow::Continue(Some(content?)),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Content {
    pub vocabulary: Vocabulary,
    pub occurrences: Vec<Bucket>,
}

type IncludedEntries = SortedVec<(EntryIndex, AttributeIndex, usize)>;
type IncludedTokens = SortedVec<TokenRef>;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Bucket {
    pub entries: IncludedEntries,
    pub tokens: IncludedTokens,
    pub occurrences: Blob<Tokens>,
}
