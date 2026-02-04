//! This will contain a simple use case of the search engine

use search_internal_helper::engine_test_util::Cache;
use search_internal_helper::{self as helper};
use test_log::test;

#[test]
fn should_index_and_search() {
    helper::init_logs();
    let mut cache = Cache::default();

    let (engine, init) = helper::create_engine();

    cache.handle_write(init).expect("init ok");

    let query = engine
        .query()
        .with_expression("'👨🏻‍❤️‍💋‍👨🏻'".parse().unwrap())
        .search();

    let found = cache.handle_search(query).expect("ok");

    insta::assert_debug_snapshot!(found, @r#"
    [
        FoundEntry {
            identifier: "emo.txt",
            matched: Value(
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
        },
    ]
    "#);
}
