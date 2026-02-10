use std::collections::BTreeMap;
use std::sync::Arc;

use maplit::btreemap;
use test_log::test;
use tracing::trace;

use crate::index::prelude::*;
use crate::index::trivial::Trivial;
use crate::query::expression::Func;
use crate::serialization::SerDes;

type Index = Trivial<Box<str>>;
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

fn insert_index_contents(index: &mut dyn IndexStore, contents: &Entries) {
    let ops = contents
        .0
        .iter()
        .flat_map(|(entry, attrs)| {
            attrs.0.iter().map(|(attr, values)| {
                IndexStoreOperation::Insert((*entry).into(), (*attr).into(), values.clone())
            })
        })
        .collect::<Vec<_>>();
    let write = index.write(0, &ops);

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
            0 => Arc::new( vec!["0".into()]),
            1 => Arc::new( vec!["128".into()]),
            2 => Arc::new( vec!["256".into()]),
        }),
        1 => Attributes(btreemap! {
            0 => Arc::new( vec!["12".into()]),
            1 => Arc::new( vec!["16".into()]),
            2 => Arc::new( vec!["24".into()]),
        }),
        2 => Attributes(btreemap! {
            0 => Arc::new( vec!["32".into()]),
            1 => Arc::new( vec!["129".into()]),
            2 => Arc::new( vec!["64".into()]),
        }),
    })
}

fn create_index() -> Index {
    let mut index = Index::default();

    insert_index_contents(&mut index, &create_index_contents());

    index
}

#[test]
fn should_create_index() {
    let index = create_index();
    insta::assert_debug_snapshot!(index, @r#"
    Trivial {
        reader: ArcSwapAny(
            None,
        ),
        writer: ArcSwapAny(
            Some(
                (
                    1,
                    {
                        "0": {
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
                            },
                        },
                        "12": {
                            AttributeIndex(
                                0,
                            ): {
                                EntryIndex(
                                    1,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                        },
                        "128": {
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
                        "129": {
                            AttributeIndex(
                                1,
                            ): {
                                EntryIndex(
                                    2,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                        },
                        "16": {
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
                            },
                        },
                        "24": {
                            AttributeIndex(
                                2,
                            ): {
                                EntryIndex(
                                    1,
                                ): {
                                    ValueIndex(
                                        0,
                                    ),
                                },
                            },
                        },
                        "256": {
                            AttributeIndex(
                                2,
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
                        "32": {
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
                        },
                        "64": {
                            AttributeIndex(
                                2,
                            ): {
                                EntryIndex(
                                    2,
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
    "#);
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
            (EntryIndex(2), AttributeIndex(0), (vec!["32".into()])),
            (EntryIndex(2), AttributeIndex(1), (vec!["129".into()])),
            (EntryIndex(2), AttributeIndex(2), (vec!["64".into()]))
        ],
        "first removal extracts associated values"
    );

    let removed = index
        .write(2, &removals)
        .filter_map(removalizer)
        .collect::<Vec<_>>();
    assert_eq!(removed, vec![], "nothing left to remove");
}

fn found(search: impl Iterator<Item = IndexSearchEvent>) -> Vec<u32> {
    let mut res = search
        .filter_map(|event| {
            trace!("event {event:?}");
            match event {
                IndexSearchEvent::Load(..) => unreachable!("must be cached"),
                IndexSearchEvent::Found(entry_index, ..) => Some(entry_index.0),
                IndexSearchEvent::Stats(_) => {
                    // ignored
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    res.sort();
    res
}

#[test]
fn should_filter_matching() {
    let index = create_index();
    let search = index
        .search(
            1,
            Some(AttributeIndex(0)),
            Func::Equals,
            &Value::text("0"),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search), vec![0]);
}

#[test]
fn should_find_matching() {
    let mut index = Index::default();

    let contents = create_index_contents();
    insert_index_contents(&mut index, &contents);

    // Every value we inserted into the index we should be able to find in it, afterwards:
    contents.walk(|entry_idx, attr_idx, value| {
        for value in value {
            let EntryValue::Tag(value) = value else {
                continue;
            };
            let mut found = vec![];
            for event in index
                .search(
                    1,
                    Some(attr_idx),
                    Func::Equals,
                    &Value::text(value.to_string()),
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
            assert_eq!(found, vec![entry_idx], "searching for {value:?}");
        }
    });
}

#[test]
fn should_filter_by_prefix() {
    let index = create_index();
    let search = index
        .search(
            1,
            Some(AttributeIndex(2)),
            Func::Prefix,
            &Value::text("2"),
            &Default::default(),
        )
        .expect("search");
    assert_eq!(found(search), vec![0, 1]);
}
