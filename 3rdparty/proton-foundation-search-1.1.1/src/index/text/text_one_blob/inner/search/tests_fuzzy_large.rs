use std::collections::BTreeSet;

use itertools::Itertools as _;
use test_log::test;

use super::*;
use crate::index::prelude::*;
use crate::index::text::trigram::Trigrams;
use crate::query::expression::TermValue;

fn words(input: &str) -> impl Iterator<Item = String> {
    input.split_whitespace().flat_map(move |token| {
        let token = token
            .replace(|c: char| !c.is_alphanumeric(), "")
            .to_lowercase();
        let has_trigrams = token
            .trigrams()
            .next()
            .map(|(_, t)| t.len())
            .unwrap_or_default()
            >= 3;
        has_trigrams.then_some(token)
    })
}
fn values(input: &str) -> EntryValues {
    words(input)
        .batching(|iter| Some(Some(EntryValue::text(iter.take(u16::MAX as usize))?)))
        .collect::<Vec<_>>()
}

#[test]
fn test_large_new_index() {
    let mut sut = TextIndex::default();
    let input = include_str!("../../../../../../../../resources/english-mobydick.txt");

    let value = values(input);
    assert!(
        sut.insert(EntryIndex(0), AttributeIndex(0), &value),
        "{value:?}"
    );

    let trigrams = sut.test_get_trigrams().collect::<BTreeSet<_>>();
    assert_eq!(trigrams.len(), 5246);
    insta::assert_debug_snapshot!(trigrams.into_iter().take(20).collect::<Vec<_>>());
    assert_eq!(sut.test_get_terms().count(), 20006);
    assert_eq!(sut.stats.length(), 893882);

    let mut data = vec![];
    ciborium::into_writer(&sut, &mut data).expect("ciborium write");
    assert_eq!(data.len(), 1538204);

    assert!(sut.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![EntryValue::text(words(
            "švestky zrály nejkulaťoulinkatější from hell"
        ))]
    ));

    let results = sut.test_search_matches("moby", 3, 0.75);
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                0.647,
            ),
            EntryIndex(
                0,
            ),
        ),
    ]
    ");

    let results = sut.test_search_matches("hello", 3, 0.75);
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                0.687,
            ),
            EntryIndex(
                0,
            ),
        ),
        (
            Score(
                0.273,
            ),
            EntryIndex(
                1,
            ),
        ),
    ]
    ");

    let results = sut.test_search_matches("nejkulaťounkatějš", 4, 0.75);
    insta::assert_debug_snapshot!(results,@r"
    [
        (
            Score(
                1.0,
            ),
            EntryIndex(
                1,
            ),
        ),
    ]
    ");

    let results = sut.test_search_matches("nonexistent", 3, 0.75);
    insta::assert_debug_snapshot!(results,@"[]");
}

#[test]
fn test_accuracy() {
    let mut sut = TextIndex::default();

    assert!(sut.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![EntryValue::text(["dmitry"])]
    ));
    assert!(sut.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![EntryValue::text(["smith"])]
    ));

    let (results, stats) = sut.search(
        &TextFilter::matches(TermValue::text("dmitry"), 3, 0.75),
        None,
        None,
        true,
        true,
    );
    insta::assert_debug_snapshot!(stats.expect("stats"), @r#"
    IndexSearchStats {
        stats: {
            AttributeIndex(
                0,
            ): IndexSearchAttributeStats {
                frequencies: {
                    Text(
                        "dmitry",
                    ): 1,
                },
                sizes: {
                    EntryIndex(
                        0,
                    ): (
                        1,
                        true,
                    ),
                    EntryIndex(
                        1,
                    ): (
                        1,
                        false,
                    ),
                },
            },
        },
    }
    "#);
    insta::assert_debug_snapshot!(results,@r#"
    {
        EntryIndex(
            0,
        ): [
            MatchedIndexTerm {
                value: Text(
                    "dmitry",
                ),
                score: Score(
                    1.0,
                ),
                positions: [
                    (
                        AttributeIndex(
                            0,
                        ),
                        ValueIndex(
                            0,
                        ),
                        TokenPosition(
                            0,
                        ),
                    ),
                ],
            },
        ],
    }
    "#);
    let res = sut.test_search_matches("dmitry", 3, 0.75);
    insta::assert_debug_snapshot!(res,@r"
    [
        (
            Score(
                0.76,
            ),
            EntryIndex(
                0,
            ),
        ),
    ]
    ");
}
