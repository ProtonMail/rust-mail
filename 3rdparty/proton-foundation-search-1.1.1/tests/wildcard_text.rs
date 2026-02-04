#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

#[path = "wildcard_common.rs"]
mod common;

use std::sync::Arc;

use common::{test, *};
use proton_foundation_search::entry::Entry;

fn tokenize(text: &str) -> EntryValue {
    EntryValue::Text(
        text.split_ascii_whitespace()
            .map(Into::into)
            .enumerate()
            .collect(),
    )
}

fn create_sut(storage: &mut Storage) -> Engine {
    let engine = Engine::builder().build();
    let mut write = engine.write().expect("write");
    write.import(entry(
        "e1",
        vec![(
            "a",
            vec![tokenize(
                "would you heed the warning if the earth was a globe",
            )],
        )],
    ));
    write.import(entry(
        "e2",
        vec![(
            "a",
            vec![tokenize(
                "global warming may accelerate the earthling evolution",
            )],
        )],
    ));
    write.import(entry(
        "e3",
        vec![("a", vec![tokenize("wonderful earring")])],
    ));
    write.import(Entry::new(
        "e4",
        [("number".into(), Arc::new(vec![EntryValue::new(5)]))].into(),
    ));
    commit(storage, write);
    engine
}

#[test]
fn search_text_prefix_exact() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // prefix matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a=glob*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "globe",
                    ),
                    "0.800",
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
                    Text(
                        "global",
                    ),
                    "0.667",
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
            .with_expression("a=globe*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "globe",
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

    // matches none because of - and exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='glo-be*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");
}

#[test]
fn search_text_prefix_fuzzy() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // prefix matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~glo*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "globe",
                    ),
                    "0.600",
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
                    Text(
                        "global",
                    ),
                    "0.500",
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
            .with_expression("a~ear*ing".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "earth",
                    ),
                    "0.500",
                    [
                        "a",
                    ],
                ),
                (
                    Text(
                        "warning",
                    ),
                    "0.714",
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
                    Text(
                        "earthling",
                    ),
                    "0.667",
                    [
                        "a",
                    ],
                ),
                (
                    Text(
                        "warming",
                    ),
                    "0.714",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e3",
            [
                (
                    Text(
                        "earring",
                    ),
                    "0.857",
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
            .with_expression("a~'glo-be*'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "globe",
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
                    Text(
                        "global",
                    ),
                    "0.667",
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
fn search_text_wild() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // matches both
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~war*ing".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "warning",
                    ),
                    "0.857",
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
                    Text(
                        "earthling",
                    ),
                    "0.556",
                    [
                        "a",
                    ],
                ),
                (
                    Text(
                        "warming",
                    ),
                    "0.857",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e3",
            [
                (
                    Text(
                        "earring",
                    ),
                    "0.714",
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
fn search_text_suffix() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // matches suffix
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~*ring".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "warning",
                    ),
                    "0.571",
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
                    Text(
                        "earthling",
                    ),
                    "0.444",
                    [
                        "a",
                    ],
                ),
                (
                    Text(
                        "warming",
                    ),
                    "0.571",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e3",
            [
                (
                    Text(
                        "earring",
                    ),
                    "0.571",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches because of fuzzy search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a~'war-ing'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [
                (
                    Text(
                        "warning",
                    ),
                    "0.857",
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
                    Text(
                        "earthling",
                    ),
                    "0.556",
                    [
                        "a",
                    ],
                ),
                (
                    Text(
                        "warming",
                    ),
                    "0.857",
                    [
                        "a",
                    ],
                ),
            ],
        ),
        (
            "e3",
            [
                (
                    Text(
                        "earring",
                    ),
                    "0.714",
                    [
                        "a",
                    ],
                ),
            ],
        ),
    ]
    "#);

    // matches none because of exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='war-ing'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");

    // matches none because of exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='war-ng'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");

    // matches none without a wildcard because of exact search
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("a='warnin'".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @"[]");
}

#[test]
fn search_text_wildcard_only() {
    let mut storage = Storage::new();
    let engine = create_sut(&mut storage);

    // fuzzy matches entries with any text content
    let found = search(
        &storage,
        engine.query().with_expression("*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [],
        ),
        (
            "e2",
            [],
        ),
        (
            "e3",
            [],
        ),
    ]
    "#);

    // exact matches entries with any text content
    let found = search(
        &storage,
        engine.query().with_expression("=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e1",
            [],
        ),
        (
            "e2",
            [],
        ),
        (
            "e3",
            [],
        ),
    ]
    "#);

    // fuzzy matches entries without any text
    let found = search(
        &storage,
        engine.query().with_expression("!*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e4",
            [],
        ),
    ]
    "#);

    // exact matches entries without any text
    let found = search(
        &storage,
        engine
            .query()
            .with_expression("!=*".parse().expect("query")),
    );

    insta::assert_debug_snapshot!(found, @r#"
    [
        (
            "e4",
            [],
        ),
    ]
    "#);
}
