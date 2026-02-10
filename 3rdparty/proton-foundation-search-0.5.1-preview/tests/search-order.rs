use proton_foundation_search::index::prelude::EntryIndex;
use proton_foundation_search::query::option::QueryOptions;
use proton_foundation_search::query::option::text::{MaximumDistance, MinimumSimilarity};
use test_log::test;

#[path = "util/test_utils.rs"]
mod test_utils;
use test_utils::SearchIndex;

#[test]
fn should_return_entry_with_sentinel() {
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");
    let mut res = index
        .search(
            "sentinel",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .collect::<Vec<_>>();
    res.sort();
    assert_eq!(res[0].1, EntryIndex(2));
}

#[test]
fn should_return_entry_with_bitwarden() {
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");
    let mut res = index
        .search(
            "bitwarden",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .collect::<Vec<_>>();
    res.sort();
    assert_eq!(res[0].1, EntryIndex(1));

    let mut res = index
        .search(
            "BITWARDEN",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .collect::<Vec<_>>();
    res.sort();
    assert_eq!(res[0].1, EntryIndex(1));
}

#[test]
fn should_return_entry_with_not() {
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");
    let mut res = index
        .search(
            "NOT",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .map(|(score, e)| (e, (100.0 * score.value()) as isize))
        .collect::<Vec<_>>();
    res.sort_by_key(|(_e, score)| -score);

    // Entry 2 has more one extra matching term, but one less exactly matching term.
    // I think it's fair to promote exact matches.
    insta::assert_debug_snapshot!(res, @r"
    [
        (
            EntryIndex(
                2,
            ),
            100,
        ),
        (
            EntryIndex(
                4,
            ),
            100,
        ),
        (
            EntryIndex(
                1,
            ),
            89,
        ),
        (
            EntryIndex(
                0,
            ),
            73,
        ),
    ]
    ");
}

// One entry has more occurrences than the other and therefore should arrive first
#[test]
fn should_return_entries_with_encryption() {
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");
    let mut res = index
        .search(
            "encryption",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .collect::<Vec<_>>();
    res.sort();
    assert_eq!(res[0].1, EntryIndex(3));
    assert_eq!(res[1].1, EntryIndex(4));
}

#[test]
fn should_return_entry_with_grands_flat() {
    // The text contains "grands-mères" which is processed as two separate terms "grands" and "mères".
    // Now we can see that "grands" appears in both subject and body attributes of the same entry.
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");

    let mut res = index
        .search(
            "grands",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .map(|(score, idx)| (idx, (100.0 * score.value()) as isize))
        .collect::<Vec<_>>();
    res.sort_by_key(|(idx, score)| (-score, *idx));

    // With harmonization, we get the maximum score per entry, not separate scores per attribute
    assert_eq!(res.as_slice(), [(EntryIndex(5), 76)]);
}

#[test]
fn should_return_entry_with_unicode() {
    let index = SearchIndex::preload("tests/fixtures/small.jsonl");
    let mut res = index
        .search(
            "Sprüngli",
            &QueryOptions::default()
                .with::<MaximumDistance>(|value| **value = 3)
                .with::<MinimumSimilarity>(|value| **value = 0.75),
        )
        .into_iter()
        .map(|(score, idx)| (idx, (100.0 * score.value()) as isize))
        .collect::<Vec<_>>();
    res.sort_by_key(|(_, score)| -score);

    assert_eq!(res.as_slice(), [(EntryIndex(5), 49)]);
}
