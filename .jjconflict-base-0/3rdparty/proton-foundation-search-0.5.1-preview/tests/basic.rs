#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! This will contain a simple use case of the search engine

use std::collections::HashSet;

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::Engine;
use proton_foundation_search::query::expression::{Expression, Func};
use search_internal_helper as helper;
use test_log::test;

use crate::storage::Storage;

#[path = "util/storage.rs"]
mod storage;

#[test]
fn should_index_and_search() {
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
        .with_expression("hello".parse().unwrap())
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<HashSet<_>, _>>()
        .expect("ok");
    assert_eq!(found.len(), 2);

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
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
                ],
            },
        },
    ]
    "#);

    let query = engine
        .query()
        .with_expression(Expression::attr(
            "title",
            Func::Matches,
            Value::text("other"),
        ))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");
    assert_eq!(
        found.len(),
        0,
        "trigram index search for 'other' is to different from 'another' so it will not match"
    );
}

#[test]
fn should_remove_from_index_and_search() {
    helper::init_logs();
    let mut storage = Storage::default();

    let engine = Engine::builder().build();

    let mut writer = engine.write().unwrap();
    writer
        .insert(
            Document::new("foo.txt")
                .with_attribute("title", Value::text("Hello World"))
                .with_attribute("creation", 12345),
        )
        .unwrap();
    writer
        .insert(
            Document::new("bar.txt")
                .with_attribute("title", Value::text("Hello Another World"))
                .with_attribute("creation", 12346),
        )
        .unwrap();

    let modified = storage
        .handle_write(writer.commit())
        .collect::<Result<HashSet<_>, _>>()
        .expect("write ok");
    assert_eq!(modified.len(), 2);

    let mut writer = engine.write().unwrap();
    writer.remove("bar.txt");

    let modified = storage
        .handle_write(writer.commit())
        .collect::<Result<HashSet<_>, _>>()
        .expect("write ok");
    assert_eq!(modified.len(), 1);

    let query = engine
        .query()
        .with_expression(Expression::attr("title", Func::Equals, Value::text("hell")))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");

    insta::assert_debug_snapshot!(found, @"[]");

    let query = engine
        .query()
        .with_expression("hello".parse().unwrap())
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
                            value: Text(
                                "hello",
                            ),
                            score: Score(
                                0.7075187496394219,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "title",
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
                ],
            },
        },
    ]
    "#);
}

#[test]
fn removes_entry() {
    let mut storage = Storage::default();
    let (engine, init) = helper::create_engine();

    let modified = storage
        .handle_write(init)
        .collect::<Result<HashSet<_>, _>>()
        .expect("init ok");
    assert_eq!(modified.len(), 3);

    let mut writer = engine.write().unwrap();
    writer.remove("foo.txt");

    let modified = storage
        .handle_write(writer.commit())
        .collect::<Result<HashSet<_>, _>>()
        .expect("removal ok");
    assert_eq!(modified.len(), 1);

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");

    assert_eq!(found.len(), 0);
}

#[test]
fn should_not_see_uncommitted_removal() {
    let mut storage = Storage::default();
    let (engine, init) = helper::create_engine();

    let modified = storage
        .handle_write(init)
        .collect::<Result<HashSet<_>, _>>()
        .expect("init ok");
    assert_eq!(modified.len(), 3);

    let mut writer = engine.write().unwrap();
    writer.remove("foo.txt");

    // The app may crash or terminate any time. Uncommitted changes shall be abandoned and cleaned up later
    drop(writer);

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<Vec<_>, _>>()
        .expect("ok");

    assert_eq!(found.len(), 1);
}
