#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! This will contain a simple use case of the search engine

use std::collections::HashSet;

use proton_foundation_search::document::Value;
use proton_foundation_search::query::expression::{Expression, Func};
use search_internal_helper as helper;
use test_log::test;

use crate::storage::Storage;

#[path = "util/storage.rs"]
mod storage;

#[test]
fn score_and_calculation() {
    helper::init_logs();
    let mut storage = Storage::default();

    let (engine, init) = helper::create_engine();

    let modified = storage
        .handle_write(init)
        .collect::<Result<HashSet<_>, _>>()
        .expect("init ok");
    assert_eq!(modified.len(), 3);

    let query = engine
        .query()
        .with_expression(Expression::And(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                Value::text("worlds"),
            ),
        ]))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matches: MatchGroup {
                operator: And,
                nodes: [
                    Value(
                        MatchValue {
                            value: Integer(
                                12345,
                            ),
                            score: Score(
                                0.7737056144690831,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "creation",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        0,
                                    ),
                                },
                            ],
                        },
                    ),
                    Value(
                        MatchValue {
                            value: Text(
                                "world",
                            ),
                            score: Score(
                                0.547839301641409,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "title",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        6,
                                    ),
                                },
                            ],
                        },
                    ),
                ],
            },
        },
    ]
    "#);
}

#[test]
fn score_or_calculation() {
    helper::init_logs();
    let mut storage = Storage::default();

    let (engine, init) = helper::create_engine();

    let modified = storage
        .handle_write(init)
        .collect::<Result<HashSet<_>, _>>()
        .expect("init ok");
    assert_eq!(modified.len(), 3);

    let query = engine
        .query()
        .with_expression(Expression::Or(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                Value::text("worlds"),
            ),
        ]))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matches: MatchGroup {
                operator: Or,
                nodes: [
                    Value(
                        MatchValue {
                            value: Integer(
                                12345,
                            ),
                            score: Score(
                                0.7737056144690831,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "creation",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        0,
                                    ),
                                },
                            ],
                        },
                    ),
                    Value(
                        MatchValue {
                            value: Text(
                                "world",
                            ),
                            score: Score(
                                0.547839301641409,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "title",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        6,
                                    ),
                                },
                            ],
                        },
                    ),
                ],
            },
        },
        FoundEntry {
            identifier: "bar.txt",
            matches: MatchGroup {
                operator: Or,
                nodes: [
                    Value(
                        MatchValue {
                            value: Text(
                                "world",
                            ),
                            score: Score(
                                0.47039604957691655,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "title",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        14,
                                    ),
                                },
                            ],
                        },
                    ),
                ],
            },
        },
    ]
    "#);
}
