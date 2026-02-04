use std::iter::from_fn;

use test_log::test;
use tracing::info;

use super::*;
use crate::document::Value;
use crate::index::prelude::*;
use crate::query::expression::{Func, TermValue};
use crate::query::results::Score;

#[test]
fn query_implements_send() {
    fn sendy(_send: impl Send) {}
    sendy(Engine::builder().build().query());
    sendy(Engine::builder().build().query().search());
}

#[test]
fn next_result_stats_passing() {
    // Here we are testing that the query execution does indeed pass stats
    // in different scenarios.
    // The detailed merging of stats shall be tested separately
    for op in [Operator::And, Operator::Or] {
        let entry_x = EntryIndex(1);
        let attr_a = AttributeIndex(0);
        let attr_b = AttributeIndex(1);
        let searches = vec![
            Box::new([ExpressionSearchEvent::Found(entry_x, MatchNode::empty())].into_iter())
                as Box<dyn Send + Iterator<Item = ExpressionSearchEvent>>,
            Box::new(
                [
                    ExpressionSearchEvent::Found(entry_x, MatchNode::empty()),
                    ExpressionSearchEvent::Stats(
                        [(
                            attr_a,
                            IndexSearchAttributeStats {
                                frequencies: [(3.into(), 4)].into(),
                                sizes: [(entry_x, (2, true))].into(),
                            },
                        )]
                        .into_iter()
                        .collect(),
                    ),
                ]
                .into_iter(),
            ),
            Box::new(
                [
                    ExpressionSearchEvent::Found(entry_x, MatchNode::empty()),
                    ExpressionSearchEvent::Stats(
                        [(
                            attr_b,
                            IndexSearchAttributeStats {
                                frequencies: [(true.into(), 4)].into(),
                                sizes: [(entry_x, (2, true))].into(),
                            },
                        )]
                        .into_iter()
                        .collect(),
                    ),
                ]
                .into_iter(),
            ),
        ];

        let sut: Box<dyn Send + Iterator<Item = ExpressionSearchEvent>> = match op {
            Operator::And => Box::new(Intersection::new(searches)),
            Operator::Or => Box::new(Union::new(searches)),
        };

        let stats = sut
            .filter_map(|event| match event {
                ExpressionSearchEvent::Load(_) => None,
                ExpressionSearchEvent::Found(_, _) => None,
                ExpressionSearchEvent::Stats(s) => Some(s),
            })
            .collect::<Vec<_>>();

        let stats = stats
            .into_iter()
            .flat_map(|s| s.into_iter())
            .collect::<Vec<_>>();

        assert_eq!(stats[0].0, attr_a);
        assert_eq!(stats[0].1.frequencies, [(Value::Integer(3), 4)].into());
        assert_eq!(stats[0].1.sizes, [(entry_x, (2, true))].into());
        assert_eq!(stats[1].0, attr_b);
        assert_eq!(stats[1].1.frequencies, [(Value::Boolean(true), 4)].into());
        assert_eq!(stats[1].1.sizes, [(entry_x, (2, true))].into());
    }
}

#[test]
fn empty_engine_search() {
    /*
     * Checking that searching with an empty collection (no attributes)
     * doesn't explode
     */
    let sut = Expression::attr("missing", Func::Matches, true);

    let collection = CollectionContent::default();

    let fake_a: Arc<dyn Index> = Arc::new(TestIndex::new([]));
    let indices = vec![(BlobId::new(0, 1), fake_a)];

    let mut search = sut.search(
        &|a| collection.get_attribute(a),
        &|| collection.get_entries().collect(),
        indices,
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
    let entry_0 = collection.insert_entry("123".into()).unwrap_or_else(|e| e);
    let entry_1 = collection.insert_entry("abc".into()).unwrap_or_else(|e| e);
    let entry_2 = collection.insert_entry("xyz".into()).unwrap_or_else(|e| e);

    let fake_a: Arc<dyn Index> = Arc::new(TestIndex::new([
        (true, entry_0, 0.11),
        (false, entry_0, 0.22),
        (true, entry_1, f64::NAN),
        (false, entry_1, 0.33),
        (true, entry_2, 0.01),
    ]));
    let indices = vec![(BlobId::new(0, 1), fake_a)];

    let mut search = sut.search(
        &|a| collection.get_attribute(a),
        &|| collection.get_entries().collect(),
        indices,
        &Default::default(),
    );

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_0 && matches.score().value() == 0.11 => {}
        other => panic!("unexpected {other:?}"),
    }

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_1 && matches.score().value() == 0.33 => {}
        other => panic!("unexpected {other:?}"),
    }
    info!("------------------------- {sut:?}");
    match search.next() {
        None => {}
        other => panic!("unexpected {other:?}"),
    }
}

/// Test index that owns its data (for Arc wrapping)
#[derive(Debug)]
struct TestIndex {
    data: Vec<(bool, EntryIndex, f64)>,
}

impl TestIndex {
    fn new(data: impl IntoIterator<Item = (bool, EntryIndex, f64)>) -> Self {
        Self {
            data: data.into_iter().collect(),
        }
    }
}

impl IndexStore for TestIndex {
    fn id(&self) -> &str {
        "test"
    }
    fn write(
        &self,
        _revision: Option<BlobId>,
        _operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        unimplemented!()
    }
}
impl IndexExport for TestIndex {
    fn export(
        &self,
        _revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        unimplemented!()
    }
}
impl IndexSearch for TestIndex {
    fn search(
        &self,
        _revision: BlobId,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        value: &TermValue,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let matches = self
            .data
            .iter()
            .copied()
            .filter_map(|(v, entry, score)| {
                if value.to_boolean() == Some(v) {
                    Some(IndexSearchEvent::Found(
                        entry,
                        vec![MatchedIndexTerm {
                            value: v.into(),
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
     * Checking that results from the indices, which is the same for the OR (disjunction)
     * are combined properly - either one is true, but all must still complete for scores
     */
    let sut = Expression::any_attr(Func::Matches, true);

    let mut collection = CollectionContent::default();
    collection.insert_attribute("attr".into());
    let entry_0 = collection.insert_entry("123".into()).unwrap_or_else(|e| e);
    let entry_1 = collection.insert_entry("abc".into()).unwrap_or_else(|e| e);
    let entry_2 = collection.insert_entry("xyz".into()).unwrap_or_else(|e| e);

    let fake_a: Arc<dyn Index> = Arc::new(Fake::new([
        Some((entry_0, f64::NAN)),
        Some((entry_1, 0.11)),
    ]));
    let fake_b: Arc<dyn Index> =
        Arc::new(Fake::new([Some((entry_1, 0.22)), Some((entry_2, 0.22))]));
    let indices = vec![(BlobId::new(0, 1), fake_a), (BlobId::new(0, 2), fake_b)];

    let mut search = sut.search(
        &|a| collection.get_attribute(a),
        &|| collection.get_entries().collect(),
        indices,
        &Default::default(),
    );

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_0 && matches.score().value() == 0.00 => {}
        other => panic!("unexpected {other:?}"),
    };

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_1 && matches.score().value() == 0.22 => {}
        other => panic!("unexpected {other:?}"),
    }

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_2 && matches.score().value() == 0.22 => {}
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
        _revision: BlobId,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        value: &TermValue,
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
                            value: value.to_string().into_boxed_str().into(),
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
        _revision: BlobId,
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
        _revision: Option<BlobId>,
        _operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
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
    let entry_0 = collection.insert_entry("123".into()).unwrap_or_else(|e| e);
    let entry_1 = collection.insert_entry("abc".into()).unwrap_or_else(|e| e);
    let entry_2 = collection.insert_entry("xyz".into()).unwrap_or_else(|e| e);

    let fake_a: Arc<dyn Index> = Arc::new(TestIndex::new([
        (true, entry_0, 1.1),
        (false, entry_0, 2.2),
        (true, entry_1, f64::NAN),
        (false, entry_1, 3.3),
        (true, entry_2, 0.1),
    ]));
    let indices = vec![(BlobId::new(0, 1), fake_a)];

    let mut search = sut.search(
        &|a| collection.get_attribute(a),
        &|| collection.get_entries().collect(),
        indices,
        &Default::default(),
    );

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_2 && matches.score().value() == 0.0 => {}
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
    let entry_0 = collection.insert_entry("123".into()).unwrap_or_else(|e| e);
    let entry_1 = collection.insert_entry("abc".into()).unwrap_or_else(|e| e);
    let entry_2 = collection.insert_entry("xyz".into()).unwrap_or_else(|e| e);

    let fake_a: Arc<dyn Index> = Arc::new(TestIndex::new([
        (true, entry_0, 1.1),
        (false, entry_0, 2.2),
        (true, entry_1, f64::NAN),
        (false, entry_1, 3.3),
        (true, entry_2, 0.1),
    ]));
    let indices = vec![(BlobId::new(0, 1), fake_a)];

    let mut search = sut.search(
        &|a| collection.get_attribute(a),
        &|| collection.get_entries().collect(),
        indices,
        &Default::default(),
    );

    match search.next() {
        Some(ExpressionSearchEvent::Found(entry, matches))
            if entry == entry_2 && matches.score().value() == 0.1 => {}
        other => panic!("unexpected {other:?}"),
    }

    match search.next() {
        None => {}
        other => panic!("unexpected {other:?}"),
    }
}
