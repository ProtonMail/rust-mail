//! This will contain a simple use case of the search engine

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::Engine;
use proton_foundation_search::query::expression::{Expression, Func, TermValue};
use search_internal_helper as helper;
use search_internal_helper::engine_test_util::Cache;
use test_log::test;

#[test]
fn should_index_and_search() {
    helper::init_logs();
    let mut storage = Cache::default();

    let (engine, init) = helper::create_engine();

    storage.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression("hello".parse().unwrap())
        .search();

    let found = storage.handle_search(query).expect("ok");
    assert_eq!(found.len(), 2);

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
        .search();

    let found = storage.handle_search(query).expect("ok");
    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Value(
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
        },
    ]
    "#);

    let query = engine
        .query()
        .with_expression(Expression::attr(
            "title",
            Func::Matches,
            TermValue::text("other"),
        ))
        .search();

    let found = storage.handle_search(query).expect("ok");
    assert_eq!(
        found.len(),
        0,
        "trigram index search for 'other' is to different from 'another' so it will not match"
    );
}

#[test]
fn should_remove_from_index_and_search() {
    helper::init_logs();
    let mut storage = Cache::default();

    let engine = Engine::builder().build();

    let mut writer = engine.write().unwrap();
    writer.insert(
        Document::new("foo.txt")
            .with_attribute("title", Value::text("Hello World"))
            .with_attribute("creation", 12345),
    );
    writer.insert(
        Document::new("bar.txt")
            .with_attribute("title", Value::text("Hello Another World"))
            .with_attribute("creation", 12346),
    );

    storage.handle_write(writer.commit()).expect("write ok");

    let mut writer = engine.write().unwrap();
    writer.remove("bar.txt");

    storage.handle_write(writer.commit()).expect("write ok");

    let query = engine
        .query()
        .with_expression(Expression::attr(
            "title",
            Func::Equals,
            TermValue::text("hell"),
        ))
        .search();

    let found = storage.handle_search(query).expect("ok");

    insta::assert_debug_snapshot!(found, @"[]");

    let query = engine
        .query()
        .with_expression("hello".parse().unwrap())
        .search();

    let found = storage.handle_search(query).expect("ok");

    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "foo.txt",
            matched: Value(
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
        },
    ]
    "#);
}

#[test]
fn removes_entry() {
    let mut storage = Cache::default();
    let (engine, init) = helper::create_engine();

    storage.handle_write(init).expect("init ok");

    let mut writer = engine.write().unwrap();
    writer.remove("foo.txt");

    storage.handle_write(writer.commit()).expect("removal ok");

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
        .search();

    let found = storage.handle_search(query).expect("ok");

    assert_eq!(found.len(), 0);
}

#[test]
fn should_not_see_uncommitted_removal() {
    let mut storage = Cache::default();
    let (engine, init) = helper::create_engine();

    storage.handle_write(init).expect("init ok");

    let mut writer = engine.write().unwrap();
    writer.remove("foo.txt");

    // The app may crash or terminate any time. Uncommitted changes shall be abandoned and cleaned up later
    drop(writer);

    let query = engine
        .query()
        .with_expression(Expression::attr("creation", Func::Equals, 12345))
        .search();

    let found = storage.handle_search(query).expect("ok");

    assert_eq!(found.len(), 1);
}
