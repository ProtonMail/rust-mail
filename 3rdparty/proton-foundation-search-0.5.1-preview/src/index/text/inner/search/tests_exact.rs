use test_log::test;

use super::*;

#[test]
fn should_search_starts_with() {
    let mut index = TextIndex::default();
    add_index_contents(&mut index);

    for (term, attr, entries) in [
        ("hello", None, vec![0, 1, 2]),
        ("world", None, vec![0, 1]),
        ("say", None, vec![1, 3]),
        ("hel", Some(AttributeIndex(0)), vec![0, 1, 2]),
        ("wan", Some(AttributeIndex(0)), vec![3]),
        ("wor", Some(AttributeIndex(1)), vec![0]),
        ("nope", None, vec![]),
    ] {
        let result = index
            .search_starts_with(&StartsWithTextFilter::new(term), attr, None)
            .map(|(result, _stats)| result)
            .into_iter()
            .flatten()
            .map(|(e, _)| e.0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        assert_eq!(result, entries, "for {term} in attr {attr:?}");
    }
}

#[test]
fn should_search_equals() {
    let mut index = TextIndex::default();
    add_index_contents(&mut index);

    for (term, attr, entries) in [
        ("hello", None, vec![0, 1, 2]),
        ("world", None, vec![0, 1]),
        ("say", None, vec![1, 3]),
        ("hel", Some(AttributeIndex(0)), vec![]),
        ("wan", Some(AttributeIndex(0)), vec![]),
        ("wor", Some(AttributeIndex(1)), vec![]),
        ("nope", None, vec![]),
    ] {
        let result = index
            .search_equals(&EqualsTextFilter::new(term), attr, None)
            .map(|(result, _stats)| result)
            .into_iter()
            .flatten()
            .map(|(e, _score)| e.0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        assert_eq!(result, entries, "for {term} in attr {attr:?}");
    }
}

fn add_index_contents(index: &mut TextIndex) {
    index.insert(
        EntryIndex(0),
        AttributeIndex(0),
        &vec![
            ["hello", "world"]
                .into_iter()
                .map(Into::into)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
    index.insert(
        EntryIndex(0),
        AttributeIndex(1),
        &vec![
            ["hello", "world"]
                .into_iter()
                .map(Into::into)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![
            ["the", "world", "say", "hello"]
                .into_iter()
                .map(Into::into)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
    index.insert(
        EntryIndex(2),
        AttributeIndex(0),
        &vec![
            ["hello"]
                .into_iter()
                .map(Into::into)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
    index.insert(
        EntryIndex(3),
        AttributeIndex(0),
        &vec![
            ["just", "wanted", "say"]
                .into_iter()
                .map(Into::into)
                .enumerate()
                .collect::<Vec<_>>()
                .into(),
        ],
    );
}
