#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

#[path = "wildcard_common.rs"]
mod common;

use std::sync::Arc;

use common::{test, *};
use proton_foundation_search::entry::Entry;

fn create_sut(storage: &mut Storage) -> Engine {
    let engine = Engine::builder().build();
    let mut write = engine.write().expect("write");

    write.import(Entry::new(
        "e1",
        [("number".into(), Arc::new(vec![EntryValue::new(5)]))].into(),
    ));
    write.import(Entry::new(
        "e2",
        [("creation".into(), Arc::new(vec![EntryValue::new(5)]))].into(),
    ));
    commit(storage, write);
    engine
}

#[test]
fn search_int_wildcard_only() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // fuzzy matches entries with any number
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("number~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Integer(
                        5,
                    ),
                    "1.000",
                    [
                        "number",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // exact matches entries with any number
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("number=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Integer(
                        5,
                    ),
                    "1.000",
                    [
                        "number",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // fuzzy matches entries without any number
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!number~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e2",
            [],
        ),
    ]
    "#);

    // exact matches entries without any number
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!number=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e2",
            [],
        ),
    ]
    "#);
}
