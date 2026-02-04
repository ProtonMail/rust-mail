use test_log::test;

use super::*;
use crate::query::expression::TermValue;

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
            .search_equals(
                &EqualsTextFilter::new(TermValue::text(term).wildcard()),
                attr,
                None,
                false,
                false,
            )
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
            .search_equals(
                &EqualsTextFilter::new(TermValue::text(term)),
                attr,
                None,
                false,
                false,
            )
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
        &vec![EntryValue::text(["hello", "world"])],
    );
    index.insert(
        EntryIndex(0),
        AttributeIndex(1),
        &vec![EntryValue::text(["hello", "world"])],
    );
    index.insert(
        EntryIndex(1),
        AttributeIndex(0),
        &vec![EntryValue::text(["the", "world", "say", "hello"])],
    );
    index.insert(
        EntryIndex(2),
        AttributeIndex(0),
        &vec![EntryValue::text(["hello"])],
    );
    index.insert(
        EntryIndex(3),
        AttributeIndex(0),
        &vec![EntryValue::text(["just", "wanted", "say"])],
    );
}
