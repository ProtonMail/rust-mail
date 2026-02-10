use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fmt::Debug;
use std::iter::{empty, from_fn};

use tracing::{info, instrument, trace};

use super::*;
use crate::document::Value;
use crate::index::collection::{CollectionContent, CollectionReadEvent};
use crate::index::prelude::{IndexSearchAttributeStats, IndexSearchEvent, MatchedIndexTerm};
use crate::query::expression::{Expression, Func, Operator};
use crate::query::option::{QueryOption, QueryOptions};
use crate::query::results::{FoundEntry, MatchGroup, MatchNode, MatchOccurrence, MatchValue};
use crate::query::stats::{AttributeStats, CollectionStats};
use crate::transaction::{Cached, LoadEvent, NoCache, TransactionState};

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
    #[wasm_bindgen(js_name = "withOptions")]
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
    state: TransactionState<NoCache<Manifest>, Manifest>,
    stage: Stage,
    options: QueryOptions,
}

enum Stage {
    Init,
    Collection(Box<dyn Send + Iterator<Item = CollectionReadEvent>>),
    Query(Box<dyn Send + Iterator<Item = QueryEvent>>),
}

impl Search {
    fn new(engine: Arc<InnerEngine>, expression: Expression, options: QueryOptions) -> Self {
        Self {
            expression,
            state: TransactionState::no_cache(MANIFEST.into(), Manifest::default),
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
            state,
            ..
        } = self;

        let manifest = match state.load()? {
            Ok(manifest) => manifest,
            Err(load) => return Some(QueryEvent::Load(load)),
        };

        loop {
            break match stage {
                Stage::Init => {
                    *stage = Stage::Collection(Box::new(
                        engine.collection.read(manifest.collection_revision),
                    ));
                    continue;
                }
                Stage::Collection(collection) => match collection.next()? {
                    CollectionReadEvent::Load(load) => Some(QueryEvent::Load(load)),
                    CollectionReadEvent::Ready(collection) => {
                        let indices = engine
                            .indices
                            .iter()
                            .map(|(id, index)| {
                                (
                                    manifest
                                        .index_revisions
                                        .get(id)
                                        .copied()
                                        .unwrap_or_default(),
                                    index.as_ref(),
                                )
                            })
                            .collect::<Vec<_>>();
                        let search =
                            self.expression
                                .search(collection.clone(), &indices, &self.options);
                        *stage = Stage::Query(search);
                        continue;
                    }
                },
                Stage::Query(search) => search.next(),
            };
        }
    }
}

impl Expression {
    /// process the query using a processor, returns true when there is any expression remaining
    pub fn process(&mut self, processor: &dyn Proc) -> bool {
        match self {
            Expression::And(expressions) | Expression::Or(expressions) => {
                expressions.retain_mut(|exp| exp.process(processor));
                !expressions.is_empty()
            }
            Expression::Not(expression) => expression.process(processor),
            // tokenize the fuzzy (Matches) search text
            Expression::Term {
                field,
                function: function @ Func::Matches,
                value,
            } => match value {
                Value::Integer(_) => true,
                Value::Boolean(_) => true,
                Value::Tag(_) => true,
                Value::Text(cow) => {
                    let mut expressions = processor
                        .process_query(cow)
                        .into_iter()
                        .map(|term| Expression::Term {
                            field: field.clone(),
                            function: *function,
                            value: Value::text(term.to_string()),
                        })
                        .collect::<Vec<_>>();

                    #[allow(clippy::expect_used, reason = "there is one item")]
                    if expressions.len() == 1 {
                        *self = expressions.pop().expect("one");
                        true
                    } else if expressions.is_empty() {
                        *self = Expression::Or(vec![]);
                        false
                    } else {
                        *self = Expression::And(expressions);
                        true
                    }
                }
            },
            Expression::Term { .. } => true,
        }
    }

    fn search(
        &self,
        collection: Cached<CollectionContent>,
        indices: &[(u64, &dyn Index)],
        options: &QueryOptions,
    ) -> Box<dyn Send + Iterator<Item = QueryEvent>> {
        match self {
            Expression::And(expressions) => {
                let searches = expressions
                    .iter()
                    .map(|exp| exp.search(collection.clone(), indices, options))
                    .collect::<Vec<_>>();
                Box::new(handle_searches(searches, Operator::And))
            }
            Expression::Or(expressions) => {
                let searches = expressions
                    .iter()
                    .map(|exp| exp.search(collection.clone(), indices, options))
                    .collect::<Vec<_>>();
                Box::new(handle_searches(searches, Operator::Or))
            }
            Expression::Not(expression) => {
                let mut remaining = collection.get_identifiers().collect::<BTreeSet<_>>();
                let mut search = expression.search(collection, indices, options);

                Box::new(from_fn(move || {
                    loop {
                        let mut next = search.next();
                        break match &mut next {
                            Some(e) => match e {
                                QueryEvent::Load(..) => {
                                    // pass
                                    next
                                }
                                QueryEvent::Found(found) => {
                                    remaining.remove(found.identifier());
                                    continue;
                                }
                                QueryEvent::Stats(..) => {
                                    // skip negates stats
                                    continue;
                                }
                            },
                            None => {
                                break remaining
                                    .pop_first()
                                    .map(|entry| QueryEvent::Found(FoundEntry::new(entry)));
                            }
                        };
                    }
                }))
            }
            Expression::Term {
                field,
                function,
                value,
            } => {
                let attribute = match field {
                    Some(field) => match collection.get_attribute(field) {
                        Some(attr) => Some(attr),
                        None => {
                            // The attribute is not present in the collection which means we would not find any matches for it anyway.
                            return Box::new(empty());
                        }
                    },
                    None => None,
                };
                let searches = indices
                    .iter()
                    .filter_map(|(revision, index)| {
                        let search =
                            index.search(*revision, attribute, *function, value, options)?;
                        let collection = collection.clone();
                        Some(Box::new(search.map(move |event| match event {
                            IndexSearchEvent::Load(load) => QueryEvent::Load(load),
                            IndexSearchEvent::Found(entry, terms) => {
                                let mut matches = BTreeMap::new();
                                for MatchedIndexTerm {
                                    positions,
                                    value,
                                    score,
                                } in terms
                                {
                                    let value: &mut MatchValue = matches
                                        .entry(value.clone())
                                        .or_insert_with(|| MatchValue::new(value, score, vec![]));
                                    value.score.merge(Operator::Or, score);
                                    value.occurrences.extend(positions.into_iter().map(
                                        |(attribute, index, position)| {
                                            MatchOccurrence::new(
                                                collection.get_attribute_name(attribute),
                                                index,
                                                position,
                                            )
                                        },
                                    ));
                                }

                                let entry = FoundEntry::new_with_matches(
                                    collection.get_identifier(entry),
                                    MatchGroup::new(
                                        Operator::Or,
                                        matches.into_values().map(MatchNode::Value),
                                    ),
                                );

                                QueryEvent::Found(entry)
                            }
                            IndexSearchEvent::Stats(index_stats) => {
                                let mut stats = CollectionStats::default();
                                for (
                                    attribute,
                                    IndexSearchAttributeStats {
                                        entries,
                                        size,
                                        frequencies,
                                        sizes,
                                    },
                                ) in index_stats
                                {
                                    let sizes = sizes
                                        .into_iter()
                                        .map(|(entry, size)| {
                                            (collection.get_identifier(entry), size)
                                        })
                                        .collect();
                                    stats += CollectionStats::new([(
                                        collection.get_attribute_name(attribute).into(),
                                        AttributeStats::new(entries, size, frequencies, sizes),
                                    )]);
                                }
                                QueryEvent::Stats(stats)
                            }
                        }))
                            as Box<dyn Send + Iterator<Item = QueryEvent>>)
                    })
                    .collect::<Vec<_>>();
                Box::new(handle_searches(searches, Operator::Or))
            }
        }
    }
}

#[instrument(skip(searches))]
fn handle_searches(
    mut searches: Vec<Box<dyn Send + Iterator<Item = QueryEvent>>>,
    operator: Operator,
) -> impl Send + Iterator<Item = QueryEvent> {
    let count = searches.len();
    trace!("handle {count} searches");
    let mut idx = count.saturating_sub(1);
    let mut finished = VecDeque::new();
    let mut pending = vec![true; count];

    // results contain a flag whether a score has been already reported and score from each index search
    let mut results = HashMap::new();

    let mut stats = CollectionStats::default();

    std::iter::from_fn(move || {
        next_result(
            &mut idx,
            &mut searches,
            &mut pending,
            &mut finished,
            &mut results,
            operator,
            &mut stats,
        )
    })
}

#[instrument(skip(searches))]
fn next_result(
    idx: &mut usize,
    searches: &mut [Box<dyn Send + Iterator<Item = QueryEvent>>],
    pending: &mut [bool],
    finished: &mut VecDeque<QueryEvent>,
    results: &mut HashMap<Box<str>, (bool, Vec<Option<FoundEntry>>)>,
    operator: Operator,
    stats: &mut CollectionStats,
) -> Option<QueryEvent> {
    let count = pending.len();
    if count == 0 {
        info!("empty search");
        return None;
    }

    trace!(count);
    loop {
        if let Some(next) = finished.pop_front() {
            break Some(next);
        }

        *idx = (*idx + 1) % count;
        let Some(event) = searches[*idx].next() else {
            // when an index completes, go through the results and report scores where this index score was missing
            // set the index as inactive/done
            pending[*idx] = false;

            let events = results.iter_mut().filter_map(|(entry, (scored, result))| {
                if !*scored {
                    handle_scored_result(pending, entry, result, operator)
                        .inspect(|_| *scored = true)
                } else {
                    None
                }
            });

            finished.extend(events);

            if finished.is_empty() && pending.iter().all(|active| !*active) {
                // all searches finished and no more results
                let stats = std::mem::take(stats);
                if stats.is_empty() || results.iter().all(|(_, (scored, ..))| !*scored) {
                    // do not produce empty stats nor stats without matches
                    break None;
                } else {
                    break Some(QueryEvent::Stats(stats));
                }
            }
            // we have some finished results or there are still some active searches
            continue;
        };

        let entry = match event {
            load @ QueryEvent::Load(_) => break Some(load),
            QueryEvent::Found(entry) => entry,
            QueryEvent::Stats(s) => {
                *stats += s;
                continue;
            }
        };

        let result = handle_result(*idx, pending, results, entry.clone(), operator);
        trace!(
            ?result,
            ?operator,
            ?idx,
            ?pending,
            ?entry,
            ?results,
            ?finished
        );

        if let Some(event) = result {
            break Some(event);
        }
    }
}

#[instrument]
fn handle_result(
    idx: usize,
    pending: &[bool],
    results: &mut HashMap<Box<str>, (bool, Vec<Option<FoundEntry>>)>,
    entry: FoundEntry,
    operator: Operator,
) -> Option<QueryEvent> {
    let identifier: Box<str> = entry.identifier().into();

    let (scored, result) = results
        .entry(identifier.clone())
        .or_insert_with(|| (false, vec![None; pending.len()]));
    result[idx] = Some(match result[idx].take() {
        Some(mut current) => {
            current.merge(operator, entry);
            current
        }
        None => entry,
    });

    if !*scored {
        // Only emit a scored event once per entry
        if let Some(event) = handle_scored_result(pending, &identifier, result, operator) {
            *scored = true;
            return Some(event);
        }
    }

    None
}

/// search_states: Some if a search is involved, true if still searching
#[instrument]
fn handle_scored_result(
    pending: &[bool],
    entry: &str,
    result: &[Option<FoundEntry>],
    operator: Operator,
) -> Option<QueryEvent> {
    let done = result.iter().enumerate().all(|(idx, found)| {
        found.is_some()
            || match operator {
                // OR: not all indices have to give a result, but have to complete for calculating scores
                Operator::Or => !pending[idx],
                // AND all indices must give a result
                Operator::And => false,
            }
    });

    if done {
        let mut entries = result.iter().flatten().cloned();
        let entry = entries.next()?;
        let entry = entries.fold(entry, |mut entry, next| {
            entry.merge(operator, next);
            entry
        });
        trace!(?entry);
        Some(QueryEvent::Found(entry))
    } else {
        trace!("not ready");
        None
    }
}

#[test]
fn next_result_or_alternation() {
    // Checking that OR results are correctly extracted in different scenarios
    let op = Operator::Or;
    for idx in 0..1 {
        for search0_pending in [false, true] {
            let mut idx = idx;
            let mut searches = [
                Box::new([].into_iter()) as Box<dyn Send + Iterator<Item = QueryEvent>>,
                Box::new(
                    [
                        QueryEvent::Found(FoundEntry::new("x")),
                        QueryEvent::Stats(CollectionStats::new([(
                            "attr_a".into(),
                            AttributeStats::new(
                                5,
                                2.0,
                                [(3.into(), 4)].into(),
                                [("x".into(), 2)].into(),
                            ),
                        )])),
                    ]
                    .into_iter(),
                ),
            ];
            let mut pending = vec![search0_pending, true];
            let mut finished = VecDeque::new();
            let mut results = HashMap::new();
            let mut stats = CollectionStats::default();

            let mut iter = std::iter::from_fn(|| {
                next_result(
                    &mut idx,
                    &mut searches,
                    &mut pending,
                    &mut finished,
                    &mut results,
                    op,
                    &mut stats,
                )
            });

            match iter.next() {
                Some(QueryEvent::Found(e)) if e.identifier() == "x" => {}
                result => panic!(
                    "expected found entry, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                ),
            }

            match iter.next() {
                Some(QueryEvent::Stats(_)) => {}
                result => panic!(
                    "expected stats, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                ),
            }

            match iter.next() {
                None => {}
                result => panic!(
                    "expected None, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                ),
            }
        }
    }
}

#[test]
fn next_result_and_alternation() {
    // Checking that AND results are correctly extracted in different scenarios
    let op = Operator::And;
    for idx in 0..1 {
        for search0_pending in [false, true] {
            let mut idx = idx;
            let mut searches = [
                Box::new(
                    search0_pending
                        .then_some(vec![QueryEvent::Found(FoundEntry::new("x"))])
                        .unwrap_or_default()
                        .into_iter(),
                ) as Box<dyn Send + Iterator<Item = QueryEvent>>,
                Box::new(
                    [
                        QueryEvent::Found(FoundEntry::new("x")),
                        QueryEvent::Stats(CollectionStats::new([(
                            "attr_a".into(),
                            AttributeStats::new(
                                5,
                                2.0,
                                [(3.into(), 4)].into(),
                                [("x".into(), 2)].into(),
                            ),
                        )])),
                    ]
                    .into_iter(),
                ),
            ];
            let mut pending = vec![search0_pending, true];
            let mut finished = VecDeque::new();
            let mut results = HashMap::new();
            let mut stats = CollectionStats::default();

            let mut iter = std::iter::from_fn(|| {
                next_result(
                    &mut idx,
                    &mut searches,
                    &mut pending,
                    &mut finished,
                    &mut results,
                    op,
                    &mut stats,
                )
            });

            if search0_pending {
                // both searches produce the same entry
                match iter.next() {
                    Some(QueryEvent::Found(e)) if e.identifier() == "x" => {}
                    result => panic!(
                        "expected found entry, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                    ),
                }

                match iter.next() {
                    Some(QueryEvent::Stats(_)) => {}
                    result => panic!(
                        "expected stats, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                    ),
                }

                match iter.next() {
                    None => {}
                    result => panic!(
                        "expected None, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                    ),
                }
            } else {
                // only one search produces the entry => no match, no stats
                match iter.next() {
                    None => {}
                    result => panic!(
                        "expected None, got {result:?} for idx: {idx}, pending: {search0_pending}, results: {results:#?}"
                    ),
                }
            }
        }
    }
}

#[test]
fn next_result_stats_merging() {
    // Here we are testing that the query execution does indeed merge stats
    // in different scenarios.
    // The detailed merging of stats shall be tested separately
    for idx in 0..1 {
        for op in [Operator::And, Operator::Or] {
            for search0_pending in [false, true] {
                let mut idx = idx;
                let mut searches = [
                    Box::new([QueryEvent::Found(FoundEntry::new("x"))].into_iter())
                        as Box<dyn Send + Iterator<Item = QueryEvent>>,
                    Box::new(
                        [
                            QueryEvent::Found(FoundEntry::new("x")),
                            QueryEvent::Stats(CollectionStats::new([(
                                "attr_a".into(),
                                AttributeStats::new(
                                    5,
                                    2.0,
                                    [(3.into(), 4)].into(),
                                    [("x".into(), 2)].into(),
                                ),
                            )])),
                        ]
                        .into_iter(),
                    ),
                    Box::new(
                        [
                            QueryEvent::Found(FoundEntry::new("x")),
                            QueryEvent::Stats(CollectionStats::new([(
                                "attr_b".into(),
                                AttributeStats::new(
                                    5,
                                    2.0,
                                    [(true.into(), 4)].into(),
                                    [("x".into(), 2)].into(),
                                ),
                            )])),
                        ]
                        .into_iter(),
                    ),
                ];
                let mut pending = vec![search0_pending, true, true];
                let mut finished = VecDeque::new();
                let mut results = HashMap::new();
                let mut stats = CollectionStats::default();

                let stats = std::iter::from_fn(|| {
                    next_result(
                        &mut idx,
                        &mut searches,
                        &mut pending,
                        &mut finished,
                        &mut results,
                        op,
                        &mut stats,
                    )
                })
                .filter_map(|event| match event {
                    QueryEvent::Load(_) => None,
                    QueryEvent::Found(_) => None,
                    QueryEvent::Stats(collection_stats) => Some(collection_stats),
                })
                .collect::<Vec<_>>();

                let [stats] = stats.as_slice() else {
                    panic!(
                        "Expected one stats event, got {stats:#?} for idx: {idx}, pending: {search0_pending}, op: {op:?}, results: {results:#?}"
                    );
                };

                assert_eq!(
                    stats.frequencies("attr_a").collect::<Vec<_>>(),
                    vec![(&Value::Integer(3), 4.0)]
                );
                assert_eq!(
                    stats.frequencies("attr_b").collect::<Vec<_>>(),
                    vec![(&Value::Boolean(true), 4.0)]
                );
                assert_eq!(
                    stats.sizes("x").collect::<Vec<_>>(),
                    vec![("attr_a", 2), ("attr_b", 2)]
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::iter::from_fn;

    use test_log::test;

    use super::*;
    use crate::index::prelude::*;
    use crate::query::expression::Func;
    use crate::query::results::Score;

    #[test]
    fn empty_engine_search() {
        /*
         * Checking that searching with an empty collection (no attributes)
         * doesn't explode
         */
        let sut = Expression::attr("missing", Func::Matches, true);

        let collection = CollectionContent::default();

        let fake_a: &[(bool, EntryIndex, f64)] = &[];
        let indices = vec![(1, &fake_a as &dyn Index)];

        let mut search = sut.search(
            Cached::new(Arc::new((1, collection))),
            indices.as_slice(),
            &Default::default(),
        );

        match search.next() {
            None => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn conjuntion_handling() {
        /*
         * Checking that results in an AND expression are combined in conjunction (all are true).
         * Since the code is shared between OR and AND, we need to focus on the differences here.
         */
        let sut = Expression::and(
            Expression::any_attr(Func::Matches, true),
            Expression::any_attr(Func::Matches, false),
        );

        let mut collection = CollectionContent::default();
        collection.insert_attribute("attr".into());
        let entry_123 = collection
            .insert_entry("123".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_abc = collection
            .insert_entry("abc".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_xyz = collection
            .insert_entry("xyz".into(), 0)
            .unwrap_or_else(|e| e);

        let fake_a = [
            (true, entry_123, 0.11),
            (true, entry_abc, f64::NAN),
            (true, entry_xyz, 0.01),
            (false, entry_123, 0.22),
            (false, entry_abc, 0.33),
        ];
        let fake_a = fake_a.as_slice();
        let indices = vec![(1, &fake_a as &dyn Index)];

        let mut search = sut.search(
            Cached::new(Arc::new((1, collection))),
            indices.as_slice(),
            &Default::default(),
        );

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "123" && entry.score().value() == 0.11 => {}
            other => panic!("unexpected {other:?}"),
        }

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "abc" && entry.score().value() == 0.33 => {}
            other => panic!("unexpected {other:?}"),
        }
        println!("------------------------- {sut:?}");
        match search.next() {
            None => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    impl IndexStore for &[(bool, EntryIndex, f64)] {
        fn id(&self) -> &str {
            "dummy"
        }
        fn write(
            &self,
            _revision: u64,
            _operations: &[IndexStoreOperation],
        ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
            unimplemented!()
        }

        fn reset(&self) {
            unimplemented!()
        }
    }
    impl IndexExport for &[(bool, EntryIndex, f64)] {
        fn export(
            &self,
            _revision: u64,
        ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
            unimplemented!()
        }
    }
    impl IndexSearch for &[(bool, EntryIndex, f64)] {
        fn search(
            &self,
            _revision: u64,
            _attribute: Option<AttributeIndex>,
            _function: Func,
            value: &Value,
            _options: &QueryOptions,
        ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
            let matches = self
                .iter()
                .copied()
                .filter_map(|(v, entry, score)| {
                    if value.to_boolean() == Some(v) {
                        Some(IndexSearchEvent::Found(
                            entry,
                            vec![MatchedIndexTerm {
                                value: value.clone(),
                                score: score.into(),
                                positions: vec![(0.into(), 0.into(), 0.into())],
                            }],
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .into_iter();

            Some(Box::new(matches))
        }
    }

    #[test]
    fn indices_handling() {
        /*
         * Checking that resultsh from the indices, which is the same for the OR (disjunction)
         * are combined properly - either one is true, but all must still complete for scores
         */
        let sut = Expression::any_attr(Func::Matches, true);

        let mut collection = CollectionContent::default();
        collection.insert_attribute("attr".into());
        let entry_123 = collection
            .insert_entry("123".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_abc = collection
            .insert_entry("abc".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_xyz = collection
            .insert_entry("xyz".into(), 0)
            .unwrap_or_else(|e| e);

        let fake_a = Fake::new([Some((entry_123, f64::NAN)), Some((entry_abc, 0.11))]);
        let fake_b = Fake::new([Some((entry_xyz, 0.22)), Some((entry_abc, 0.22))]);
        let indices = vec![(1, &fake_a as &dyn Index), (1, &fake_b)];

        let mut search = sut.search(
            Cached::new(Arc::new((1, collection))),
            indices.as_slice(),
            &Default::default(),
        );

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "abc" && entry.score().value() == 0.22 => {}
            other => panic!("unexpected {other:?}"),
        }

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "xyz" && entry.score().value() == 0.22 => {}
            other => panic!("unexpected {other:?}"),
        };

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "123" && entry.score().value() == 0.00 => {}
            other => panic!("unexpected {other:?}"),
        };

        match search.next() {
            None => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[derive(Debug)]
    struct Fake {
        queue: Vec<Option<(EntryIndex, Score)>>,
    }

    impl Fake {
        fn new(queue: impl IntoIterator<Item = Option<(EntryIndex, f64)>>) -> Self {
            Self {
                queue: queue
                    .into_iter()
                    .map(|item| item.map(|(e, score)| (e, score.into())))
                    .collect(),
            }
        }
    }
    impl IndexSearch for Fake {
        fn search(
            &self,
            _revision: u64,
            _attribute: Option<AttributeIndex>,
            _function: Func,
            value: &Value,
            _options: &QueryOptions,
        ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
            let mut events = self
                .queue
                .iter()
                .map(|event| {
                    event.map(|(entry, score)| {
                        IndexSearchEvent::Found(
                            entry,
                            vec![MatchedIndexTerm {
                                value: value.clone(),
                                score,
                                positions: vec![(0.into(), 0.into(), 0.into())],
                            }],
                        )
                    })
                })
                .collect::<VecDeque<_>>();
            Some(Box::new(from_fn(move || events.pop_front().flatten())))
        }
    }
    impl IndexExport for Fake {
        fn export(
            &self,
            _revision: u64,
        ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
            unimplemented!()
        }
    }
    impl IndexStore for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn write(
            &self,
            _revision: u64,
            _operations: &[IndexStoreOperation],
        ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
            unimplemented!()
        }
        fn reset(&self) {
            unimplemented!()
        }
    }

    #[test]
    fn negation_handling() {
        /*
         * Checking that results in a NOT expression are inverted (excluded).
         */
        let sut = Expression::not(Expression::any_attr(Func::Matches, false));

        let mut collection = CollectionContent::default();
        collection.insert_attribute("attr".into());
        let entry_123 = collection
            .insert_entry("123".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_abc = collection
            .insert_entry("abc".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_xyz = collection
            .insert_entry("xyz".into(), 0)
            .unwrap_or_else(|e| e);

        let fake_a = [
            (true, entry_123, 1.1),
            (true, entry_abc, f64::NAN),
            (true, entry_xyz, 0.1),
            (false, entry_123, 2.2),
            (false, entry_abc, 3.3),
        ];
        let fake_a = fake_a.as_slice();
        let indices = vec![(1, &fake_a as &dyn Index)];

        let mut search = sut.search(
            Cached::new(Arc::new((1, collection))),
            indices.as_slice(),
            &Default::default(),
        );

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "xyz" && entry.score().value() == 0.0 => {}
            other => panic!("unexpected {other:?}"),
        }

        match search.next() {
            None => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn negation_filtering() {
        /*
         * Checking that results in a AND(... NOT ...) expression are filtered according to the exclusion matches.
         */
        let sut = Expression::and(
            Expression::not(Expression::any_attr(Func::Matches, false)),
            Expression::any_attr(Func::Matches, true),
        );

        let mut collection = CollectionContent::default();
        collection.insert_attribute("attr".into());
        let entry_123 = collection
            .insert_entry("123".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_abc = collection
            .insert_entry("abc".into(), 0)
            .unwrap_or_else(|e| e);
        let entry_xyz = collection
            .insert_entry("xyz".into(), 0)
            .unwrap_or_else(|e| e);

        let fake_a = [
            (true, entry_123, 1.1),
            (true, entry_abc, f64::NAN),
            (true, entry_xyz, 0.1),
            (false, entry_123, 2.2),
            (false, entry_abc, 3.3),
        ];
        let fake_a = fake_a.as_slice();
        let indices = vec![(1, &fake_a as &dyn Index)];

        let mut search = sut.search(
            Cached::new(Arc::new((1, collection))),
            indices.as_slice(),
            &Default::default(),
        );

        match search.next() {
            Some(QueryEvent::Found(entry))
                if entry.identifier() == "xyz" && entry.score().value() == 0.1 => {}
            other => panic!("unexpected {other:?}"),
        }

        match search.next() {
            None => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
