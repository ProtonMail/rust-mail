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
        [("draft".into(), Arc::new(vec![EntryValue::new(true)]))].into(),
    ));
    write.import(Entry::new(
        "e2",
        [("read".into(), Arc::new(vec![EntryValue::new(false)]))].into(),
    ));
    commit(storage, write);
    engine
}

#[test]
fn search_int_wildcard_only() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // fuzzy matches entries with any value
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("draft~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Boolean(
                        true,
                    ),
                    "1.000",
                    [
                        "draft",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // exact matches entries with any value
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("draft=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Boolean(
                        true,
                    ),
                    "1.000",
                    [
                        "draft",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // fuzzy matches entries without any value
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!draft~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e2",
            [],
        ),
    ]
    "#);

    // exact matches entries without any value
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!draft=*".parse().expect("query")),
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
