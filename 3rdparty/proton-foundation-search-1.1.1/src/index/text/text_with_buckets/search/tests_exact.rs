use test_log::test;

use super::*;
use crate::index::text::TextIndex as TextIndexWithBuckets;
use crate::query::expression::{Func, TermValue};
use crate::query::option::QueryOptions;
use crate::query::option::results::{CollectPositions, CollectStats};

#[test]
fn should_search_starts_with() {
    let index = TextIndexWithBuckets::default();
    let mut cache = BTreeMap::new();
    let mut rev = None;
    test_util::add_index_contents(&index, &mut rev, &mut cache);

    let options = QueryOptions::default()
        .with(|o: &mut CollectPositions| *o = false.into())
        .with(|o: &mut CollectStats| *o = false.into());

    for (term, attr, entries) in [
        ("hello", None, vec![0, 1, 2]),
        ("world", None, vec![0, 1]),
        ("say", None, vec![1, 3]),
        ("hel", Some(AttributeIndex(0)), vec![0, 1, 2]),
        ("wan", Some(AttributeIndex(0)), vec![3]),
        ("wor", Some(AttributeIndex(1)), vec![0]),
        ("nope", None, vec![]),
    ] {
        let search = index
            .search(
                rev.expect("revision"),
                attr,
                Func::Equals,
                &TermValue::text(term).wildcard(),
                &options,
            )
            .expect("search");

        let results = test_util::handle_search(search, &cache, &mut Default::default())
            .map(|(e, _)| e.0)
            .collect::<Vec<_>>();

        assert_eq!(results, entries, "for {term} in attr {attr:?}");
    }
}

#[test]
fn should_search_equals() {
    let index = TextIndexWithBuckets::default();
    let mut cache = BTreeMap::new();
    let mut rev = None;
    test_util::add_index_contents(&index, &mut rev, &mut cache);

    let options = QueryOptions::default()
        .with(|o: &mut CollectPositions| *o = false.into())
        .with(|o: &mut CollectStats| *o = false.into());

    for (term, attr, entries) in [
        ("hello", None, vec![0, 1, 2]),
        ("world", None, vec![0, 1]),
        ("say", None, vec![1, 3]),
        ("hel", Some(AttributeIndex(0)), vec![]),
        ("wan", Some(AttributeIndex(0)), vec![]),
        ("wor", Some(AttributeIndex(1)), vec![]),
        ("nope", None, vec![]),
    ] {
        let search = index
            .search(
                rev.expect("revision"),
                attr,
                Func::Equals,
                &TermValue::text(term),
                &options,
            )
            .expect("search");

        let result = test_util::handle_search(search, &cache, &mut Default::default())
            .map(|(e, _)| e.0)
            .collect::<Vec<_>>();

        assert_eq!(result, entries, "for {term} in attr {attr:?}");
    }
}
