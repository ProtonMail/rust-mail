mod bools;
mod ints;
mod tags;

use std::sync::Arc;

use tracing::trace;

use super::*;
use crate::blob::{BlobId, Cached};
use crate::index::prelude::{IndexStore, IndexStoreEvent, IndexStoreOperation};
use crate::serialization::SerDes;

type Values = Arc<EntryValues>;

#[derive(Clone)]
struct Attributes(BTreeMap<u32, Values>);

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

impl<V> Trivial<V>
where
    V: IndexableValue,
    EntryValue: From<V>,
    V: for<'v> TryFrom<&'v EntryValue>,
{
    fn test_fill(
        &self,
        revision: &mut Option<BlobId>,
        cache: &mut BTreeMap<BlobId, (String, Cached)>,
        entries: &Entries,
    ) {
        insert_index_contents(self, revision, cache, entries);
    }
}

fn format_cache(cache: &BTreeMap<BlobId, (String, Cached)>) -> String {
    cache
        .iter()
        .map(|(id, (json, _))| format!("{id:?}: {json}\n"))
        .collect()
}

fn insert_index_contents(
    index: &dyn IndexStore,
    revision: &mut Option<BlobId>,
    cache: &mut BTreeMap<BlobId, (String, Cached)>,
    contents: &Entries,
) {
    let ops = contents
        .0
        .iter()
        .flat_map(|(entry, attrs)| {
            attrs.0.iter().map(|(attr, values)| {
                IndexStoreOperation::Insert((*entry).into(), (*attr).into(), values.clone())
            })
        })
        .collect::<Vec<_>>();
    for event in index.write(*revision, &ops) {
        handle_write(event, revision, cache);
    }
}

fn handle_write(
    event: IndexStoreEvent,
    revision: &mut Option<BlobId>,
    cache: &mut BTreeMap<BlobId, (String, Cached)>,
) -> Option<(EntryIndex, AttributeIndex, Vec<Option<EntryValue>>)> {
    match event {
        IndexStoreEvent::Removed { entry, attr, value } => Some((entry, attr, value)),
        IndexStoreEvent::Inserted { entry, attr } => Some((entry, attr, vec![])),
        IndexStoreEvent::Load(load) => {
            match cache.get(&load.id()) {
                Some((_, cached)) => {
                    load.send_cached(cached).expect("send");
                }
                None => unreachable!("data should be cached"),
            }
            None
        }
        IndexStoreEvent::Save(save) => {
            let id = save.id();
            let cached = save.recv();
            let data = cached.serialize(&SerDes::JsonPretty).expect("serialize");
            let data = String::from_utf8(data).expect("json");
            cache.insert(id, (data, cached));
            None
        }
        IndexStoreEvent::Release(release) => {
            cache.insert(
                release.id(),
                ("released".into(), Cached::new("released", Arc::new(()))),
            );
            None
        }
        IndexStoreEvent::Revision(revision_event) => {
            *revision = Some(revision_event.id());
            None
        }
    }
}

fn found(
    search: impl Iterator<Item = IndexSearchEvent>,
    cache: &BTreeMap<BlobId, (String, Cached)>,
) -> Vec<u32> {
    let mut res = search
        .filter_map(|event| {
            trace!("event {event:?}");
            match event {
                IndexSearchEvent::Load(load) => {
                    if let Some((_, cached)) = cache.get(&load.id()) {
                        load.send_cached(cached).expect("send cached");
                    } else {
                        unreachable!("must be cached");
                    }
                    None
                }
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
fn insertes_and_removes() {
    let sut = Trivial::<u64>::default();
    let transaction = sut.write(
        None,
        [
            IndexStoreOperation::Insert(
                EntryIndex(0),
                AttributeIndex(0),
                Arc::new(
                    [0, 1, 2]
                        .into_iter()
                        .map(Into::into)
                        .map(Some)
                        .collect::<Vec<_>>(),
                ),
            ),
            IndexStoreOperation::Insert(
                EntryIndex(1),
                AttributeIndex(1),
                Arc::new(
                    [1, 2, 3]
                        .into_iter()
                        .map(Into::into)
                        .map(Some)
                        .collect::<Vec<_>>(),
                ),
            ),
            IndexStoreOperation::Remove(EntryIndex(0)),
        ]
        .as_slice(),
    );

    let mut inserted = vec![];
    let mut removed = vec![];
    let mut saves = String::new();
    let mut revisions = vec![];
    for event in transaction {
        match event {
            IndexStoreEvent::Inserted { entry, attr } => inserted.push((entry, attr)),
            IndexStoreEvent::Removed { entry, attr, value } => removed.push((entry, attr, value)),
            IndexStoreEvent::Load(load) => {
                unreachable!("should not load, got {load:?}")
            }
            IndexStoreEvent::Save(save) => {
                let id = save.id();
                let data = save.recv().serialize(&SerDes::JsonPretty).expect("json");
                saves += &format!("{id}: {}\n", String::from_utf8(data).expect("json string"));
            }
            IndexStoreEvent::Release(release) => {
                unreachable!("should not release, got {release:?}")
            }
            IndexStoreEvent::Revision(revision_event) => revisions.push(revision_event.id()),
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
            [0, 1, 2]
                .into_iter()
                .map(Into::into)
                .map(Some)
                .collect::<Vec<_>>()
        )]
    );

    insta::assert_debug_snapshot!(revisions, @r"
    [
        260DF74E5DB95DB4A3DB015BC3BD0969,
    ]
    ");

    insta::assert_snapshot!(saves, @r#"
    260DF74E5DB95DB4A3DB015BC3BD0969: {
      "1": {
        "1": {
          "1": [
            0
          ]
        }
      },
      "2": {
        "1": {
          "1": [
            1
          ]
        }
      },
      "3": {
        "1": {
          "1": [
            2
          ]
        }
      }
    }
    "#);
}
