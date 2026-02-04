#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::Engine;
use test_log::test;

#[cfg(test)]
#[test]
fn sans_io_engine() {
    use search_internal_helper::engine_test_util::{commit, query};

    let mut storage = BTreeMap::new();

    let sut = Engine::builder().build();

    let mut write = sut.write().expect("single writer");
    write.insert(
        Document::new("123")
            .with_attribute("text", Value::text("Hello! Killin' in"))
            .with_attribute("text", Value::text("the name of"))
            .with_attribute("int", 321)
            .with_attribute("int", 42)
            .with_attribute("bool", true)
            .with_attribute("tag", Value::tag("daring"))
            .with_attribute("tag", Value::tag("words")),
    );
    write.insert(
        Document::new("456")
            .with_attribute("text", Value::text("Hello! Like the wild ones,"))
            .with_attribute("text", Value::text(" we seek shelter"))
            .with_attribute("int", 654)
            .with_attribute("int", 42)
            .with_attribute("bool", false)
            .with_attribute("tag", Value::tag("longing"))
            .with_attribute("tag", Value::tag("words")),
    );

    commit(&mut storage, write.commit());

    let result = query(
        &storage,
        sut.query()
            .with_expression("hello".parse().expect("query"))
            .search(),
    )
    .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(result, @r#"
    [
        (
            "123",
            "0.778",
        ),
        (
            "456",
            "0.608",
        ),
    ]
    "#);
}
