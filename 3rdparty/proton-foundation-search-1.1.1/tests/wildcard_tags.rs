#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

#[path = "wildcard_common.rs"]
mod common;

use common::{test, *};

fn create_sut(storage: &mut Storage) -> Engine {
    let engine = Engine::builder().build();
    let mut write = engine.write().expect("write");
    write.import(entry(
        "e1",
        vec![("a", vec![EntryValue::Tag("abc/def/ghi".into())])],
    ));
    write.import(entry(
        "e2",
        vec![("a", vec![EntryValue::Tag("abc/de/fghi".into())])],
    ));
    write.import(entry(
        "e3",
        vec![("b", vec![EntryValue::Tag("other/tag".into())])],
    ));
    commit(storage, write);
    engine
}

#[test]
fn search_tag_prefix_exact() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // prefix matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='abc/de*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // more specific prefix matches one
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='abc/def/*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches none because of // and exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='abc/def//*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");
}

#[test]
fn search_tag_prefix_fuzzy() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // prefix matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/de*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // more specific prefix matches one
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/def/*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches despite // because of fuzzy search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/def//*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);
}

#[test]
fn search_tag_wild() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/de*ghi'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches only one
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/de*fghi'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches both because of fuzzy search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/de*/ghi'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches only one because of exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='abc/de*/ghi'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches none without a wildcard
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='abc/de'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");

    // matches none without a wildcard
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'abc/de'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"[]"#);
}

#[test]
fn search_tag_wildcard_only_fuzzy() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // matches entries tagged in attribute "a"
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches only one that doesn't have any tag for attr "a"
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!a~*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e3",
            [],
        ),
    ]
    "#);
}

#[test]
fn search_tag_wildcard_only_exact() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // matches entries tagged in attribute "a"
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Tag(
                        "abc/def/ghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e2",
            [
                (
                    Tag(
                        "abc/de/fghi",
                    ),
                    "1.000",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches only one that doesn't have any tag for attr "a"
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!a=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e3",
            [],
        ),
    ]
    "#);
}
