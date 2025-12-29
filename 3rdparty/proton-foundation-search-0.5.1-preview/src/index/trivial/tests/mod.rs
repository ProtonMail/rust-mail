#![allow(clippy::expect_used)]

mod bools;
mod ints;
mod tags;

use super::*;
use crate::index::prelude::{IndexStore, IndexStoreEvent, IndexStoreOperation};
use crate::serialization::SerDes;
use crate::transaction::{LoadEvent, ReleaseEvent, SaveEvent};

#[test]
fn insertes_and_removes() {
    let sut = Trivial::<u64>::default();
    let transaction = sut.write(
        0,
        [
            IndexStoreOperation::Insert(
                EntryIndex(0),
                AttributeIndex(0),
                Arc::new([0, 1, 2].into_iter().map(Into::into).collect::<Vec<_>>()),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(1),
                Arc::new([1, 2, 3].into_iter().map(Into::into).collect::<Vec<_>>()),
            ),
            IndexStoreOperation::Remove(EntryIndex(0)),
        ]
        .as_slice(),
    );

    let mut blob = vec![];
    let mut inserted = vec![];
    let mut removed = vec![];
    let mut loads = vec![];
    let mut saves = vec![];
    let mut releases = vec![];
    for event in transaction {
        match event {
            IndexStoreEvent::Inserted { entry, attr } => inserted.push((entry, attr)),
            IndexStoreEvent::Removed { entry, attr, value } => removed.push((entry, attr, value)),
            IndexStoreEvent::Load(LoadEvent { name, send }) => {
                send(&SerDes::Json, blob.clone()).expect("empty data send");
                loads.push(name);
            }
            IndexStoreEvent::Save(SaveEvent { name, recv }) => {
                let data = recv(&SerDes::Json).expect("json");
                blob = data.clone();
                saves.push((name, String::from_utf8(data).expect("json string")));
            }
            IndexStoreEvent::Release(ReleaseEvent { name }) => {
                releases.push(name);
            }
        }
    }

    assert_eq!(
        inserted,
        vec![
            (EntryIndex(0), AttributeIndex(0)),
            (EntryIndex(1), AttributeIndex(1))
        ]
    );
    assert_eq!(
        removed,
        vec![(
            EntryIndex(0),
            AttributeIndex(0),
            [0, 1, 2].into_iter().map(Into::into).collect::<Vec<_>>()
        )]
    );

    insta::assert_debug_snapshot!(loads, @r#"
    [
        "u64 r0",
    ]
    "#);
    insta::assert_debug_snapshot!(saves, @r#"
    [
        (
            "u64 r1",
            "[1,{\"1\":{\"1\":{\"1\":[0]}},\"2\":{\"1\":{\"1\":[1]}},\"3\":{\"1\":{\"1\":[2]}}}]",
        ),
    ]
    "#);
    insta::assert_debug_snapshot!(releases, @r#"
    [
        "u64 r0",
    ]
    "#);
}
