use std::collections::VecDeque;
use std::fmt::Debug;
use std::ops::ControlFlow;

use tracing::instrument;

use super::*;
use crate::blob::{Blob, LoadEvent};
use crate::engine::query::execution::ExpressionSearchEvent;
use crate::index::collection::{CollectionContent, CollectionReadEvent};
use crate::index::prelude::IndexSearchStats;
use crate::query::expression::{Expression, Func, Operator, TermValue, TermValuePart};
use crate::query::option::{QueryOption, QueryOptions};
use crate::query::results::{FoundEntry, MatchNode};
use crate::query::stats::CollectionStats;

mod execution;
mod process_expression;
mod process_value;

#[cfg(test)]
mod process_value_tests;

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Creates a query builder for this engine
    pub fn query(&self) -> Query {
        Query::new(self.inner.clone())
    }
}

/// Search engine query event
#[derive(Debug)]
pub enum QueryEvent {
    /// The engine request a blob load from the app. Once loaded, call send.
    /// Query result iterator shall stop if the loaded blob is not sent. IOW, `next()` will return `None`.
    Load(LoadEvent),
    /// Matched a document identifier.
    Found(FoundEntry),
    /// Collections statistics collected
    Stats(CollectionStats),
}

/// A query search builder prepares an engine query for execution
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Query {
    engine: Arc<InnerEngine>,
    expression: Expression,
    options: QueryOptions,
}

impl Query {
    #[instrument(skip_all)]
    fn new(engine: Arc<InnerEngine>) -> Self {
        Self {
            engine,
            expression: Expression::And(vec![]),
            options: QueryOptions::default(),
        }
    }

    /// Add a search expression condition in conjunction (AND) with any previous ones
    pub fn with_expression(mut self, mut query: Expression) -> Self {
        if query.process(self.engine.processor.as_ref()) {
            self.expression.push(query);
        }
        self
    }

    /// Query expression
    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    /// Mutable query expression
    pub fn expression_mut(&mut self) -> &mut Expression {
        &mut self.expression
    }

    /// Update a query option
    pub fn with_option<O: QueryOption + Default>(mut self, update: impl FnOnce(&mut O)) -> Self {
        (update)(self.options.get_mut());
        self
    }

    /// Mutable query options
    pub fn options_mut(&mut self) -> &mut QueryOptions {
        &mut self.options
    }

    /// Query options
    pub fn options(&self) -> &QueryOptions {
        &self.options
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Query {
    /// Update all query options
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withOptions")
    )]
    pub fn with_options(mut self, options: QueryOptions) -> Self {
        self.options.extend(options);
        self
    }

    /// Execute the query search
    pub fn search(self) -> Search {
        Search::new(self.engine.clone(), self.expression, self.options)
    }
}

/// A running engine query search is an iterator of [`QueryEvent`]s
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Search {
    expression: Expression,
    engine: Arc<InnerEngine>,
    blob: Blob<Manifest>,
    stage: Stage,
    options: QueryOptions,
}

enum Stage {
    Init,
    Collection(Box<dyn Send + Iterator<Item = CollectionReadEvent>>),
    Query(
        Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
        Arc<CollectionContent>,
        CollectionStats,
    ),
}

impl Search {
    fn new(engine: Arc<InnerEngine>, expression: Expression, options: QueryOptions) -> Self {
        Self {
            expression,
            blob: Blob::new(MANIFEST_BLOB_ID, MANIFEST),
            stage: Stage::Init,
            engine,
            options,
        }
    }
}

impl Iterator for Search {
    type Item = QueryEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            engine,
            stage,
            blob,
            ..
        } = self;

        let manifest = match blob.load() {
            ControlFlow::Continue(manifest) => manifest,
            ControlFlow::Break(load) => return Some(QueryEvent::Load(load)),
        };

        loop {
            break match stage {
                Stage::Init => {
                    *stage = Stage::Collection(Box::new({
                        let coll = engine.collection.clone();
                        manifest
                            .collection_revision
                            .into_iter()
                            .flat_map(move |revision| coll.read(revision))
                    }));
                    continue;
                }
                Stage::Collection(collection) => match collection.next()? {
                    CollectionReadEvent::Load(load) => Some(QueryEvent::Load(load)),
                    CollectionReadEvent::Ready(collection) => {
                        // Collect indices as Arc for cloneable access in lazy search creation
                        let indices: Vec<_> = engine
                            .indices
                            .iter()
                            .filter_map(|(id, index)| {
                                Some((*manifest.index_revisions.get(id)?, Arc::clone(index)))
                            })
                            .collect();
                        let entries = || collection.get_entries().collect();
                        let attributes = |attr: &str| collection.get_attribute(attr);
                        let search =
                            self.expression
                                .search(&attributes, &entries, indices, &self.options);
                        *stage = Stage::Query(search, collection, CollectionStats::default());
                        continue;
                    }
                },
                Stage::Query(search, collection, stats) => match search.next() {
                    None => {
                        if stats.is_empty() {
                            None
                        } else {
                            Some(QueryEvent::Stats(core::mem::take(stats)))
                        }
                    }
                    Some(event) => match event {
                        ExpressionSearchEvent::Load(load_event) => {
                            Some(QueryEvent::Load(load_event))
                        }
                        ExpressionSearchEvent::Found(entry_index, match_node) => {
                            let id = collection.get_identifier(entry_index);
                            let match_node = match_node
                                .convert(&|attr| collection.get_attribute_name(attr).into());

                            Some(QueryEvent::Found(FoundEntry::new_with_matches(
                                id, match_node,
                            )))
                        }
                        ExpressionSearchEvent::Stats(index_search_stats) => {
                            stats.add(
                                index_search_stats,
                                |e| collection.get_identifier(e),
                                |a| collection.get_attribute_name(a).into(),
                            );
                            continue;
                        }
                    },
                },
            };
        }
    }
}
