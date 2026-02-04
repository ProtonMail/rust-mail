use std::sync::Arc;

use maplit::btreemap;
use test_log::test;

use super::*;
use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::query::expression::Func;

type Index = Trivial<u64>;

fn create_index_contents() -> Entries {
    Entries(btreemap! {
        0 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(0.into())]),
            1 => Arc::new(vec![Some(128.into())]),
            2 => Arc::new(vec![Some(256.into())]),
        }),
        1 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(12.into())]),
            1 => Arc::new(vec![Some(16.into())]),
            2 => Arc::new(vec![Some(24.into())]),
        }),
        2 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(32.into())]),
            1 => Arc::new(vec![Some(127.into())]),
            2 => Arc::new(vec![Some(64.into())]),
        }),
    })
}

#[test]
fn should_create_index() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    insta::assert_snapshot!(format_cache(&cache), @r#"
    56F4D4AB380354D6A4FE084DAAD3AFB2: {
      "0": {
        "0": {
          "0": [
            0
          ]
        }
      },
      "12": {
        "0": {
          "1": [
            0
          ]
        }
      },
      "16": {
        "1": {
          "1": [
            0
          ]
        }
      },
      "24": {
        "2": {
          "1": [
            0
          ]
        }
      },
      "32": {
        "0": {
          "2": [
            0
          ]
        }
      },
      "64": {
        "2": {
          "2": [
            0
          ]
        }
      },
      "127": {
        "1": {
          "2": [
            0
          ]
        }
      },
      "128": {
        "1": {
          "0": [
            0
          ]
        }
      },
      "256": {
        "2": {
          "0": [
            0
          ]
        }
      }
    }
    "#);
}

#[test]
fn should_move_entry() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let removals = [IndexStoreOperation::Remove(EntryIndex(2))];

    let mut removed = sut
        .write(revision, &removals)
        .filter_map(|event| handle_write(event, &mut revision, &mut cache))
        .collect::<Vec<_>>();

    removed.sort();
    assert_eq!(
        removed,
        vec![
            (EntryIndex(2), AttributeIndex(0), vec![Some(32.into())]),
            (EntryIndex(2), AttributeIndex(1), vec![Some(127.into())]),
            (EntryIndex(2), AttributeIndex(2), vec![Some(64.into())])
        ],
        "first removal extracts associated values"
    );

    let removed = sut
        .write(revision, &removals)
        .filter_map(|event| handle_write(event, &mut revision, &mut cache))
        .collect::<Vec<_>>();
    assert_eq!(removed, vec![], "nothing left to remove");
}

#[test]
fn should_filter_with_no_equals() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::Equals,
            &42.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache).len(), 0);
}

#[test]
fn should_filter_with_equal() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::Equals,
            &0.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache), vec![0]);
}

#[test]
fn should_filter_with_missing_grater_than() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::GreaterThan,
            &42.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache).len(), 0);
}

#[test]
fn should_filter_with_matching_grater_than() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::GreaterThan,
            &0.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache), vec![1, 2]);
}

#[test]
fn should_filter_with_missing_grater_than_or_equal() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::GreaterThanOrEqual,
            &42.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache).len(), 0);
}

#[test]
fn should_filter_with_matching_grater_than_or_equal() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(0)),
            Func::GreaterThanOrEqual,
            &0.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache), vec![0, 1, 2]);
}

#[test]
fn should_filter_with_missing_less_than() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(1)),
            Func::LessThan,
            &10.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache).len(), 0);
}

#[test]
fn should_filter_with_matching_less_than() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(1)),
            Func::LessThan,
            &127.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache), vec![1]);
}

#[test]
fn should_filter_with_missing_less_than_or_equal() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(1)),
            Func::LessThanOrEqual,
            &5.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache).len(), 0);
}

#[test]
fn should_filter_with_matching_less_than_or_equal() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &create_index_contents());

    let search = sut
        .search(
            revision.expect("revision"),
            Some(AttributeIndex(1)),
            Func::LessThanOrEqual,
            &128.into(),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search, &cache), vec![0, 1, 2]);
}

#[test]
fn should_find_matching() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let sut = Index::default();
    let entries = create_index_contents();
    sut.test_fill(&mut revision, &mut cache, &entries);

    // Every value we inserted into the index we should be able to find in it, afterwards:
    entries.walk(|entry_idx, attr_idx, value| {
        for value in value {
            let Some(EntryValue::Integer(value)) = value else {
                continue;
            };
            let search = sut
                .search(
                    revision.expect("revision"),
                    Some(attr_idx),
                    Func::Equals,
                    &(*value).into(),
                    &Default::default(),
                )
                .expect("query");
            let found = found(search, &cache);

            assert_eq!(found, vec![entry_idx.0]);
        }
    });
}
