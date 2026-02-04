use std::collections::VecDeque;
use std::ops::ControlFlow;

use super::inner::filter::TextFilter;
use super::search::filter::{TextFilterSansIo, TextSearch};
use super::*;
use crate::blob::{Blob, BlobId};
use crate::index::prelude::*;
use crate::query::expression::{Func, TermValue};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};

/// Text filtering and search query processing
pub mod filter;

impl IndexSearch for TextIndexSansIo {
    fn search(
        &self,
        revision: BlobId,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &TermValue,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let filter = match function {
            Func::Matches => TextSearch {
                filter: TextFilter::matches(
                    value.clone(),
                    MaximumDistance::get(options),
                    MinimumSimilarity::get(options),
                ),
                attribute,
            },
            Func::Equals => TextSearch {
                filter: TextFilter::equals(value.clone()),
                attribute,
            },
            Func::LessThan
            | Func::LessThanOrEqual
            | Func::GreaterThan
            | Func::GreaterThanOrEqual => return None,
        };
        let collect_stats = CollectStats::get(options);
        let collect_positions = CollectPositions::get(options);

        Some(Box::new(Finder::new(
            self,
            revision,
            filter,
            collect_stats,
            collect_positions,
        )))
    }
}

#[derive(Default)]
enum Finder<F> {
    Loading {
        filter: F,
        blob: Blob<TextIndex>,
        collect_stats: bool,
        collect_postitions: bool,
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
    fn new(
        _index: &TextIndexSansIo,
        revision: BlobId,
        filter: F,
        collect_stats: bool,
        collect_postitions: bool,
    ) -> Self {
        Self::Loading {
            filter,
            blob: Blob::new(revision, NAME),
            collect_stats,
            collect_postitions,
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
                Finder::Loading {
                    filter,
                    blob,
                    collect_stats,
                    collect_postitions,
                } => {
                    match blob.load() {
                        ControlFlow::Continue(index) => {
                            // we have loaded and will just iterate now
                            *self = Self::Iterating {
                                results: filter
                                    .get(index, collect_stats, collect_postitions)
                                    .collect(),
                            };
                            continue;
                        }
                        ControlFlow::Break(load) => {
                            // still loading, preserve self as is
                            *self = Self::Loading {
                                filter,
                                blob,
                                collect_stats,
                                collect_postitions,
                            };
                            Some(IndexSearchEvent::Load(load))
                        }
                    }
                }
            };
        }
    }
}
