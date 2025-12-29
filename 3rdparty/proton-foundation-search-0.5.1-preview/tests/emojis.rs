#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! This will contain a simple use case of the search engine

use std::collections::HashSet;

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
        .with_expression("'👨🏻‍❤️‍💋‍👨🏻'".parse().unwrap())
        .search();

    let found = storage
        .handle_search(query)
        .collect::<Result<HashSet<_>, _>>()
        .expect("ok");
    assert_eq!(found.len(), 1);

    insta::assert_debug_snapshot!(found, @r#"
    {
        FoundEntry {
            identifier: "emo.txt",
            matches: MatchGroup {
                operator: Or,
                nodes: [
                    Value(
                        MatchValue {
                            value: Text(
                                "👨🏻\u{200d}❤\u{fe0f}\u{200d}💋\u{200d}👨🏻",
                            ),
                            score: Score(
                                0.6315964199747618,
                            ),
                            occurrences: [
                                MatchOccurrence {
                                    attribute: "title",
                                    index: ValueIndex(
                                        0,
                                    ),
                                    position: TokenPosition(
                                        18,
                                    ),
                                },
                            ],
                        },
                    ),
                ],
            },
        },
    }
    "#);
}
