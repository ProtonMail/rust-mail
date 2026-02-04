use std::sync::Arc;

use maplit::btreemap;
use test_log::test;

use super::*;
use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::query::expression::Func;

type Index = Trivial<bool>;

fn create_index_contents() -> Entries {
    Entries(btreemap! {
        0 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(true.into())]),
            1 => Arc::new(vec![Some(true.into())]),
        }),
        1 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(true.into())]),
            1 => Arc::new(vec![Some(false.into())]),
        }),
        2 => Attributes(btreemap! {
            0 => Arc::new(vec![Some(false.into())]),
            1 => Arc::new(vec![Some(false.into())]),
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
    FE3816C5C176558D8A723AD6B492C493: {
      "false": {
        "0": {
          "2": [
            0
          ]
        },
        "1": {
          "1": [
            0
          ],
          "2": [
            0
          ]
        }
      },
      "true": {
        "0": {
          "0": [
            0
          ],
          "1": [
            0
          ]
        },
        "1": {
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
            (EntryIndex(2), AttributeIndex(0), vec![Some(false.into())]),
            (EntryIndex(2), AttributeIndex(1), vec![Some(false.into())])
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
fn should_find_matching() {
    let mut cache = BTreeMap::new();
    let mut revision = None;
    let entries = create_index_contents();
    let sut = Index::default();
    sut.test_fill(&mut revision, &mut cache, &entries);

    // Every value we inserted into the index we should be able to find in it, afterwards:
    entries.walk(|entry_idx, attr_idx, value| {
        for value in value {
            let Some(EntryValue::Boolean(value)) = value else {
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

            assert!(found.contains(&entry_idx.0));
        }
    });
}
