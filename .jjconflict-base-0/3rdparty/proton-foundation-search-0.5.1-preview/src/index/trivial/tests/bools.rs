use std::collections::BTreeMap;
use std::sync::Arc;

use maplit::btreemap;
use test_log::test;

use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::query::expression::Func;
use crate::serialization::SerDes;

type Index = Trivial<bool>;
type Values = Arc<EntryValues>;

#[derive(Clone)]
struct Attributes(BTreeMap<u8, Values>);

#[derive(Clone)]
struct Entries(BTreeMap<u32, Attributes>);

impl Entries {
    fn walk<F>(&self, mut f: F)
    where
        F: FnMut(EntryIndex, AttributeIndex, &EntryValues),
    {
        for (&entry_idx, entry) in &self.0 {
            let entry_idx = EntryIndex(entry_idx);
            for (&attr_idx, value) in &entry.0 {
                let attr_idx = AttributeIndex(attr_idx);
                f(entry_idx, attr_idx, value);
            }
        }
    }
}

fn insert_index_contents(index: &mut dyn IndexStore, rev: u64, contents: &Entries) {
    let ops = contents
        .0
        .iter()
        .flat_map(|(entry, attrs)| {
            attrs.0.iter().map(|(attr, values)| {
                IndexStoreOperation::Insert((*entry).into(), (*attr).into(), values.clone())
            })
        })
        .collect::<Vec<_>>();
    let write = index.write(rev, &ops);

    for event in write {
        match event {
            IndexStoreEvent::Inserted { .. } => {}
            IndexStoreEvent::Removed { .. } => {}
            IndexStoreEvent::Load(load_event) => {
                (load_event.send)(&SerDes::Json, vec![]).expect("send");
            }
            IndexStoreEvent::Save(save_event) => {
                (save_event.recv)(&SerDes::Json).expect("recv");
            }
            IndexStoreEvent::Release(..) => {}
        }
    }
}

fn create_index_contents() -> Entries {
    Entries(btreemap! {
        0 => Attributes(btreemap! {
            0 => Arc::new(vec![true.into()]),
            1 => Arc::new(vec![true.into()]),
        }),
        1 => Attributes(btreemap! {
            0 => Arc::new(vec![true.into()]),
            1 => Arc::new(vec![false.into()]),
        }),
        2 => Attributes(btreemap! {
            0 => Arc::new(vec![false.into()]),
            1 => Arc::new(vec![false.into()]),
        }),
    })
}

fn create_index() -> Index {
    let mut index = Index::default();

    insert_index_contents(&mut index, 0, &create_index_contents());

    index
}

#[test]
fn should_create_index() {
    let index = create_index();
    insta::assert_debug_snapshot!(index, @r"
    Trivial {
        reader: ArcSwapAny(
            None,
        ),
        writer: ArcSwapAny(
            Some(
                (
                    1,
                    {
                        false: {
                            AttributeIndex(
                                0,
                            ): {
                                EntryIndex(
                                    2,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                            AttributeIndex(
                                1,
                            ): {
                                EntryIndex(
                                    1,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                                EntryIndex(
                                    2,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                        },
                        true: {
                            AttributeIndex(
                                0,
                            ): {
                                EntryIndex(
                                    0,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                                EntryIndex(
                                    1,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                            AttributeIndex(
                                1,
                            ): {
                                EntryIndex(
                                    0,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                        },
                    },
                ),
            ),
        ),
    }
    ");
}

#[test]
fn should_move_entry() {
    let index = create_index();

    let removalizer = |event| match event {
        IndexStoreEvent::Removed { entry, attr, value } => Some((entry, attr, value)),
        IndexStoreEvent::Inserted { .. } => {
            unreachable!("not inserting")
        }
        IndexStoreEvent::Load(..) => {
            unreachable!("data should be cached")
        }
        IndexStoreEvent::Save(..) | IndexStoreEvent::Release(..) => {
            // ignoring these
            None
        }
    };

    let removals = [IndexStoreOperation::Remove(EntryIndex(2))];

    let mut removed = index
        .write(1, &removals)
        .filter_map(removalizer)
        .collect::<Vec<_>>();

    removed.sort();
    assert_eq!(
        removed,
        vec![
            (EntryIndex(2), AttributeIndex(0), vec![false.into()]),
            (EntryIndex(2), AttributeIndex(1), vec![false.into()])
        ],
        "first removal extracts associated values"
    );

    let removed = index
        .write(2, &removals)
        .filter_map(removalizer)
        .collect::<Vec<_>>();
    assert_eq!(removed, vec![], "nothing left to remove");
}

#[test]
fn should_find_matching() {
    let index = create_index();
    let contents = create_index_contents();

    // Every value we inserted into the index we should be able to find in it, afterwards:
    contents.walk(|entry_idx, attr_idx, value| {
        for value in value {
            let EntryValue::Boolean(value) = value else {
                continue;
            };
            let mut found = vec![];
            for event in index
                .search(
                    1,
                    Some(attr_idx),
                    Func::Equals,
                    &Value::Boolean(*value),
                    &Default::default(),
                )
                .expect("query")
            {
                match event {
                    IndexSearchEvent::Load(..) => {
                        unreachable!("should be cached")
                    }
                    IndexSearchEvent::Found(entry_index, ..) => {
                        found.push(entry_index);
                    }
                    IndexSearchEvent::Stats(_) => {
                        // ignored
                    }
                }
            }
            assert!(found.contains(&entry_idx));
        }
    });
}
