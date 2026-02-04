#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::Engine;
use search_internal_helper::engine_test_util::{commit, query};
use test_log::test;

#[test]
fn tag_prefix_search() {
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
            .with_attribute("tag", Value::tag("words:short:abc")),
    );
    write.insert(
        Document::new("456")
            .with_attribute("text", Value::text("Hello! Like the wild ones,"))
            .with_attribute("text", Value::text(" we seek shelter"))
            .with_attribute("int", 654)
            .with_attribute("int", 42)
            .with_attribute("bool", false)
            .with_attribute("tag", Value::tag("longing"))
            .with_attribute("tag", Value::tag("words:short:xyz")),
    );

    commit(&mut storage, write.commit());

    let query_expr = "tag~'words:short:*'";

    let result = query(
        &storage,
        sut.query()
            .with_expression(query_expr.parse().expect("query"))
            .search(),
    )
    .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(result, @r#"
    [
        (
            "123",
            "0.76",
        ),
        (
            "456",
            "0.76",
        ),
    ]
    "#);

    let query_expr2 = "tag='words:short:abc'";

    let result = query(
        &storage,
        sut.query()
            .with_expression(query_expr2.parse().expect("query"))
            .search(),
    )
    .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(result, @r#"
    [
        (
            "123",
            "0.76",
        ),
    ]
    "#);
}
