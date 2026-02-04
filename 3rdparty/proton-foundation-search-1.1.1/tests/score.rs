//! This will contain a simple use case of the search engine

use proton_foundation_search::query::expression::{Expression, Func, TermValue};
use proton_foundation_search::query::option::results::{CollectPositions, CollectStats};
use search_internal_helper::engine_test_util::Cache;
use search_internal_helper::{self as helper};
use test_log::test;

#[test]
fn no_stats() {
    // let's see that we can run search without stats (saving time)

    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression(Expression::And(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                TermValue::text("worlds"),
            ),
        ]))
        //.with_option(|o: &mut CollectPositions| *o = false.into())
        .with_option(|o: &mut CollectStats| *o = false.into())
        .search();

    let found = cache.handle_search(query).expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Group(
                MatchGroup {
                    operator: And,
                    nodes: [
                        Value(
                            MatchValue {
                                value: Integer(
                                    12345,
                                ),
                                score: Score(
                                    1.0,
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
                                    0.8333333333333334,
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
            ),
        },
    ]
    "#);
}

#[test]
fn score_and_calculation() {
    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression(Expression::And(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                TermValue::text("worlds"),
            ),
        ]))
        .search();

    let found = cache.handle_search(query).expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Group(
                MatchGroup {
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
                                    0.7304524021885453,
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
            ),
        },
    ]
    "#);
}

#[test]
fn score_or_calculation() {
    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression(Expression::Or(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                TermValue::text("worlds"),
            ),
        ]))
        .search();

    let found = cache.handle_search(query).expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Group(
                MatchGroup {
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
                                    0.7304524021885453,
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
            ),
        },
        FoundEntry {
            identifier: "bar.txt",
            matched: Value(
                MatchValue {
                    value: Text(
                        "world",
                    ),
                    score: Score(
                        0.6271947327692221,
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
        },
    ]
    "#);
}

#[test]
fn no_positions() {
    // let's see that we can run search without stats and matched values (saving time)

    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression(Expression::And(vec![
            Expression::attr("creation", Func::Equals, 12345),
            Expression::any_attr(
                Func::Matches,
                // "worlds" is not in the text, but similar to "world"
                TermValue::text("worlds"),
            ),
        ]))
        .with_option(|o: &mut CollectPositions| *o = false.into())
        .with_option(|o: &mut CollectStats| *o = false.into())
        .search();

    let found = cache.handle_search(query).expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Group(
                MatchGroup {
                    operator: And,
                    nodes: [
                        Group(
                            MatchGroup {
                                operator: Or,
                                nodes: [],
                            },
                        ),
                        Value(
                            MatchValue {
                                value: Text(
                                    "world",
                                ),
                                score: Score(
                                    0.8333333333333334,
                                ),
                                occurrences: [],
                            },
                        ),
                    ],
                },
            ),
        },
    ]
    "#);
}
