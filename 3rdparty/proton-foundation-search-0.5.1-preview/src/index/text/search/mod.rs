use std::collections::VecDeque;

use tracing::trace;

use super::*;
use crate::index::prelude::*;
use crate::index::text::inner::filter::TextFilter;
use crate::index::text::search::filter::{TextFilterSansIo, TextSearch};
use crate::query::expression::Func;
use crate::query::option::QueryOptions;
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};
use crate::transaction::{Read, TransactionState};

/// Text filtering and search query processing
pub mod filter;

impl IndexSearch for TextIndexSansIo {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let filter = match function {
            Func::Matches => TextSearch {
                filter: TextFilter::matches(
                    value.to_string(),
                    MaximumDistance::get(options),
                    MinimumSimilarity::get(options),
                ),
                attribute,
            },
            Func::Equals => TextSearch {
                filter: TextFilter::equals(value.to_string()),
                attribute,
            },
            Func::Prefix => TextSearch {
                filter: TextFilter::starts_with(value.to_string()),
                attribute,
            },
            Func::LessThan
            | Func::LessThanOrEqual
            | Func::GreaterThan
            | Func::GreaterThanOrEqual => return None,
        };
        Some(Box::new(Finder::new(revision, self, filter)))
    }
}

#[derive(Default)]
enum Finder<F> {
    Loading {
        filter: F,
        state: TransactionState<Read<TextIndex>, TextIndex>,
    },
    Iterating {
        results: VecDeque<IndexSearchEvent>,
    },
    #[default]
    Done,
}

impl<F> Finder<F>
where
    F: TextFilterSansIo,
{
    fn new(revision: u64, index: &TextIndexSansIo, filter: F) -> Self {
        trace!(cache="writer", rev=?index.writer.load_full().map(|a| a.0));
        trace!(cache="reader", rev=?index.reader.load_full().map(|a| a.0));
        Self::Loading {
            filter,
            state: TransactionState::read(
                revision,
                NAME.into(),
                index.writer.load_full(),
                index.reader.clone(),
            ),
        }
    }
}

impl<F> Iterator for Finder<F>
where
    F: TextFilterSansIo,
{
    type Item = IndexSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            break match std::mem::take(self) {
                Finder::Done => None,
                Finder::Iterating { mut results } => {
                    let next = results.pop_front();
                    *self = Finder::Iterating { results };
                    next
                }
                Finder::Loading { filter, mut state } => {
                    match state.load()? {
                        Ok(index) => {
                            // we have loaded and will just iterate now
                            *self = Self::Iterating {
                                results: filter.get(index).collect(),
                            };
                            continue;
                        }
                        Err(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading { filter, state };
                            Some(IndexSearchEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}
