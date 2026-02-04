use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::iter::from_fn;
use std::ops::ControlFlow;

use test_log::test;
use tracing::info;

use super::*;
use crate::blob::{Blob, Cached, LoadEvent, RevisionEvent, SaveEvent};
use crate::document::Document;
use crate::index::prelude::*;
use crate::query::expression::{Func, TermValue};
use crate::query::option::QueryOptions;
use crate::serialization::SerDes;

#[derive(Debug, Clone, Copy)]
enum FakeStoreEvent {
    Load(BlobId),
    Save(BlobId),
    Revision(BlobId),
    #[allow(dead_code)]
    Removed,
    Inserted,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum FakeSearchEvent {
    Load(BlobId),
    Matched,
}

#[derive(Debug, Default)]
struct FakeIndex {
    writes: Vec<Option<FakeStoreEvent>>,
    reads: Vec<Option<FakeSearchEvent>>,
}

struct FakeIter<E>(VecDeque<Option<E>>);

impl IndexSearch for FakeIndex {
    fn search(
        &self,
        revision: BlobId,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        _value: &TermValue,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        Some(Box::new(FakeIter(
            self.reads
                .iter()
                .copied()
                .map(|e| {
                    e.map(move |e| match e {
                        FakeSearchEvent::Load(id) => IndexSearchEvent::Load(LoadEvent::new(
                            id,
                            format!("{id} {revision}").into_boxed_str(),
                            Box::new(move |_| {
                                Ok(Cached::new(format!("{id} {revision}"), Arc::new(())))
                            }),
                        )),
                        FakeSearchEvent::Matched => IndexSearchEvent::Found(EntryIndex(0), vec![]),
                    })
                })
                .collect(),
        )))
    }
}
impl IndexExport for FakeIndex {
    fn export(
        &self,
        _revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        unimplemented!()
    }
}
impl IndexStore for FakeIndex {
    fn id(&self) -> &str {
        "fake"
    }
    fn write(
        &self,
        revision: Option<BlobId>,
        _operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        Box::new(FakeIter(
            self.writes
                .iter()
                .copied()
                .map(|e| {
                    e.map(move |e| match e {
                        FakeStoreEvent::Load(id) => IndexStoreEvent::Load(LoadEvent::new(
                            id,
                            format!("{id} {revision:?}").into_boxed_str(),
                            Box::new(move |_| {
                                Ok(Cached::new(format!("{id} {revision:?}"), Arc::new(())))
                            }),
                        )),
                        FakeStoreEvent::Save(id) => IndexStoreEvent::Save(SaveEvent::new(
                            id,
                            Cached::new(format!("{id} {revision:?}"), Arc::new(())),
                        )),
                        FakeStoreEvent::Revision(id) => {
                            IndexStoreEvent::Revision(RevisionEvent::new(id))
                        }
                        FakeStoreEvent::Removed => IndexStoreEvent::Removed {
                            entry: EntryIndex(0),
                            attr: AttributeIndex(0),
                            value: vec![EntryValue::new(0)],
                        },
                        FakeStoreEvent::Inserted => IndexStoreEvent::Inserted {
                            entry: EntryIndex(0),
                            attr: AttributeIndex(0),
                        },
                    })
                })
                .collect(),
        ))
    }
}

impl<E> Iterator for FakeIter<E> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop_front()?
    }
}

#[test]
fn tracks_blob_versions() {
    let mut storage: BTreeMap<BlobId, Vec<u8>> = BTreeMap::new();

    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_index(FakeIndex {
            reads: vec![],
            writes: vec![
                Some(FakeStoreEvent::Load(BlobId::new(0, 1))),
                Some(FakeStoreEvent::Inserted),
                Some(FakeStoreEvent::Save(BlobId::new(0, 2))),
                Some(FakeStoreEvent::Save(BlobId::new(0, 3))),
                Some(FakeStoreEvent::Revision(BlobId::new(0, 3))),
            ],
        })
        .build();

    let mut write = sut.write().expect("one writer");
    write.insert(Document::new("xyz").with_attribute("pkey", 123));
    let write = write.commit();

    let results = write
        .map(|event| match event {
            WriteEvent::Load(event) => {
                let id = event.id();
                if let Some(data) = storage.get(&id) {
                    event.send(&SerDes::Json, data).expect("send");
                } else {
                    event.send_empty().expect("empty send");
                }
                format!("load {id}")
            }
            WriteEvent::Save(event) => {
                let id = event.id();
                let data = event.recv().serialize(&SerDes::Json);
                if let Ok(ref data) = data {
                    storage.insert(id, data.clone());
                }
                format!(
                    "save {id} {}",
                    data.map(|v| String::from_utf8(v)
                        .unwrap_or_else(|e| format!("{:?}", e.into_bytes())))
                        .unwrap_or_else(|e| format!("Error: {e}"))
                )
            }
            WriteEvent::Stats(stats) => {
                format!("stats {stats:?}")
            }
        })
        .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(results, @r#"
    [
        "load 00000000000000000000000000000000",
        "save E952A48FAC1055C3B23529068DA75989 {\"attributes\":[\"pkey\"],\"entries\":{\"xyz\":0},\"identifiers\":{\"0\":\"xyz\"}}",
        "load 00000000000000000000000000000001",
        "save 00000000000000000000000000000002 null",
        "save 00000000000000000000000000000003 null",
        "save 00000000000000000000000000000000 {\"collection_revision\":\"E952A48FAC1055C3B23529068DA75989\",\"index_revisions\":{\"fake\":\"00000000000000000000000000000003\"},\"active_blobs\":[\"00000000000000000000000000000002\",\"00000000000000000000000000000003\",\"E952A48FAC1055C3B23529068DA75989\"],\"released_blobs\":[]}",
        "stats Stats { documents: 1 }",
    ]
    "#);
}

#[test]
#[should_panic(expected = "load event was not handled")]
fn panics_when_not_loaded() {
    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_index(FakeIndex {
            reads: vec![],
            writes: vec![
                Some(FakeStoreEvent::Load(BlobId::new(11, 11))),
                Some(FakeStoreEvent::Inserted),
                Some(FakeStoreEvent::Save(BlobId::new(11, 22))),
                Some(FakeStoreEvent::Save(BlobId::new(22, 22))),
            ],
        })
        .build();

    let write = sut.write().expect("single writer").commit();

    for _event in write {
        // not handling load leads to panic
    }
}

#[test]
fn concurrent_read_write_transactions() {
    let mut storage: BTreeMap<BlobId, String> = BTreeMap::new();

    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_index(IdentityIndex::default())
        .build();

    // initial write to lift revisions
    let mut write = sut.write().expect("single writer");
    write.insert(Document::new("abc").with_attribute("pkey", 42));
    write.insert(Document::new("bug").with_attribute("pkey", 24));
    let write = write.commit();
    for event in write {
        info!(?event, message = "writes");
        match event {
            WriteEvent::Load(load) => {
                if let Some(data) = storage.get(&load.id()) {
                    load.send(&SerDes::Json, data.as_bytes()).expect("send");
                } else {
                    load.send_empty().expect("empty send");
                }
            }
            WriteEvent::Save(save) => {
                storage.insert(
                    save.id(),
                    String::from_utf8(save.recv().serialize(&SerDes::Json).expect("recv"))
                        .expect("str"),
                );
            }
            WriteEvent::Stats(_) => {}
        }
    }

    insta::assert_debug_snapshot!(storage, @r#"
    {
        00000000000000000000000000000000: "{\"collection_revision\":\"23485A701913582BBD12F20B53189A48\",\"index_revisions\":{\"identity\":\"5B46FE66B3E45BE99B1AF792338604E2\"},\"active_blobs\":[\"23485A701913582BBD12F20B53189A48\",\"5B46FE66B3E45BE99B1AF792338604E2\"],\"released_blobs\":[]}",
        23485A701913582BBD12F20B53189A48: "{\"attributes\":[\"pkey\"],\"entries\":{\"abc\":0,\"bug\":1},\"identifiers\":{\"0\":\"abc\",\"1\":\"bug\"}}",
        5B46FE66B3E45BE99B1AF792338604E2: "[0,1]",
    }
    "#);

    // now we interleave a write with query
    let mut write = sut.write().expect("single writer");
    write.remove("bug");
    let mut write = write.commit();
    match write.next().expect("next") {
        WriteEvent::Load(load) if load.id() == MANIFEST_BLOB_ID => {
            if let Some(data) = storage.get(&load.id()) {
                load.send(&SerDes::Json, data.as_bytes()).expect("send");
            } else {
                load.send_empty().expect("empty send");
            }
        }
        otherwise => panic!("expected manifest load, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Load(load) if load.id().to_string() == "23485A701913582BBD12F20B53189A48" => {
            if let Some(data) = storage.get(&load.id()) {
                load.send(&SerDes::Json, data.as_bytes()).expect("send");
            } else {
                load.send_empty().expect("empty send");
            }
        }
        otherwise => panic!("expected collection load, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Save(save) if save.id().to_string() == "B05DB4235B8552DFB5A649B472B23B11" => {
            storage.insert(
                save.id(),
                String::from_utf8(save.recv().serialize(&SerDes::Json).expect("recv"))
                    .expect("str"),
            );
        }
        otherwise => panic!("expected collection save, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Load(load) if load.id().to_string() == "5B46FE66B3E45BE99B1AF792338604E2" => {
            if let Some(data) = storage.get(&load.id()) {
                load.send(&SerDes::Json, data.as_bytes()).expect("send");
            } else {
                load.send_empty().expect("empty send");
            }
        }
        otherwise => panic!("expected identity load, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Save(save) if save.id().to_string() == "F7B3C10D3F8F5727B740B7EFBF8AE97E" => {
            storage.insert(
                save.id(),
                String::from_utf8(save.recv().serialize(&SerDes::Json).expect("recv"))
                    .expect("str"),
            );
        }
        otherwise => panic!("expected identity save, got {otherwise:?}"),
    }

    // query just before the write is finisheds
    let query = sut
        .query()
        .with_expression("ignored".parse().expect("query"))
        .search();
    let mut record = vec![];
    for event in query {
        info!(?event, message = "reads");
        match event {
            QueryEvent::Load(load) => {
                let id = load.id();
                if let Some(data) = storage.get(&load.id()) {
                    load.send(&SerDes::Json, data.as_bytes()).expect("send");
                } else {
                    load.send_empty().expect("empty send");
                }
                record.push(format!("loaded {id}"));
            }
            QueryEvent::Found(found) => record.push(format!("found {:?}", found.identifier())),
            QueryEvent::Stats(_) => record.push("stats".to_string()),
        }
    }

    // the query is not affected by uncommitted write transaction
    insta::assert_debug_snapshot!(record, @r#"
    [
        "loaded 00000000000000000000000000000000",
        "loaded 23485A701913582BBD12F20B53189A48",
        "loaded 5B46FE66B3E45BE99B1AF792338604E2",
        "found \"abc\"",
        "found \"bug\"",
    ]
    "#);

    // then we complete the write transaction
    match write.next().expect("next") {
        WriteEvent::Save(save) if save.id() == MANIFEST_BLOB_ID => {
            storage.insert(
                save.id(),
                String::from_utf8(save.recv().serialize(&SerDes::Json).expect("recv"))
                    .expect("str"),
            );
        }
        otherwise => panic!("expected manifest save, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Stats(_) => {}
        otherwise => panic!("expected stats, got {otherwise:?}"),
    }
    match write.next() {
        None => {}
        otherwise => panic!("expected None, got {otherwise:?}"),
    }

    // the query should now see the written changes
    let query = sut
        .query()
        .with_expression("ignored".parse().expect("query"))
        .search();
    let mut record = vec![];
    for event in query {
        match event {
            QueryEvent::Load(load) => {
                let id = load.id();
                if let Some(data) = storage.get(&load.id()) {
                    load.send(&SerDes::Json, data.as_bytes()).expect("send");
                } else {
                    load.send_empty().expect("empty send");
                }
                record.push(format!("loaded {id}"));
            }
            QueryEvent::Found(found) => record.push(format!("found {:?}", found.identifier())),
            QueryEvent::Stats(_) => record.push("stats".to_string()),
        }
    }

    // the query is not affected by uncommitted write transaction
    insta::assert_debug_snapshot!(record, @r#"
    [
        "loaded 00000000000000000000000000000000",
        "loaded B05DB4235B8552DFB5A649B472B23B11",
        "loaded F7B3C10D3F8F5727B740B7EFBF8AE97E",
        "found \"abc\"",
    ]
    "#);
}

/// A minimal index implementation, just recording EntryIds
#[derive(Debug, Default)]
struct IdentityIndex {}

// Type alias to reduce complexity
type IdentityContent = BTreeSet<EntryIndex>;

impl IndexStore for IdentityIndex {
    fn id(&self) -> &str {
        "identity"
    }
    fn write(
        &self,
        revision: Option<BlobId>,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        let mut blob = revision.map(|revision| Blob::<IdentityContent>::new(revision, "identity"));
        let mut index = None;
        let mut ops = operations.iter().cloned().collect::<VecDeque<_>>();
        let mut modified = false;
        let mut events = VecDeque::new();

        Box::new(from_fn(move || {
            let index = match &mut index {
                Some(index) => index,
                None => match blob.as_mut().map(Blob::fetch_and_free) {
                    Some(ControlFlow::Break(load)) => return Some(IndexStoreEvent::Load(load)),
                    Some(ControlFlow::Continue(cached)) => index.insert(cached.as_ref().clone()),
                    None => index.insert(IdentityContent::default()),
                },
            };

            loop {
                if let Some(event) = events.pop_front() {
                    break Some(event);
                }

                break Some(if let Some(op) = ops.pop_front() {
                    match op {
                        IndexStoreOperation::Insert(entry, attr, ..) => {
                            if index.insert(entry) {
                                modified = true;
                                IndexStoreEvent::Inserted { entry, attr }
                            } else {
                                continue;
                            }
                        }
                        IndexStoreOperation::Remove(entry) => {
                            if index.remove(&entry) {
                                modified = true;
                                IndexStoreEvent::Removed {
                                    entry,
                                    attr: AttributeIndex(0),
                                    value: EntryValues::default(),
                                }
                            } else {
                                continue;
                            }
                        }
                    }
                } else if std::mem::take(&mut modified) {
                    let revision = BlobId::generate(&*index);
                    let (release, save) = blob
                        .get_or_insert_with(|| Blob::new(revision, "identity"))
                        .save(revision, Arc::new(index.clone()));
                    let revision = IndexStoreEvent::Revision(RevisionEvent::new(revision));
                    events.extend(release.map(IndexStoreEvent::Release));
                    events.push_back(revision);
                    IndexStoreEvent::Save(save)
                } else {
                    return None;
                });
            }
        }))
    }
}
impl IndexExport for IdentityIndex {
    fn export(
        &self,
        _revision: BlobId,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        unimplemented!()
    }
}
impl IndexSearch for IdentityIndex {
    fn search(
        &self,
        revision: BlobId,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        _value: &TermValue,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let blob = Blob::<IdentityContent>::new(revision, "identity");
        let mut found = None;
        Some(Box::new(from_fn(move || {
            let index = match blob.load() {
                ControlFlow::Continue(index) => index,
                ControlFlow::Break(load) => return Some(IndexSearchEvent::Load(load)),
            };

            let found = found.get_or_insert_with(|| index.iter().copied().collect::<VecDeque<_>>());

            let found = found.pop_front()?;

            Some(IndexSearchEvent::Found(found, vec![]))
        })))
    }
}

#[test]
fn concurrent_writes_not_allowed() {
    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_default_indices()
        .build();

    // initial write to lift revisions
    let write_1 = sut.write().expect("first writer");

    assert!(sut.write().is_none());

    let events = write_1.commit();

    assert!(sut.write().is_none());

    drop(events);

    let _write_2 = sut.write().expect("second writer");
}

#[test]
fn unique_index_ids() {
    // Only one index with a given ID can be configured for the engine

    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_default_indices();
    assert!(sut.with_text_index().is_err());

    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_boolean_index();
    let sut = sut.with_text_index().expect("another index");
    assert!(sut.with_boolean_index().is_err());

    let sut = Engine::builder()
        .with_builtin_processor(&Default::default())
        .with_index(FakeIndex::default());
    assert!(sut.with_index(FakeIndex::default()).is_err());
}

#[test]
fn remove_absent_entry() {
    let engine = Engine::builder().build();

    let mut write = engine.write().expect("write");
    write.remove("missing");

    // Removing an entry that was not inserted used to panic.
    // Here we assert that it's OK and no write is actually needed.
    for event in write.commit() {
        match event {
            WriteEvent::Load(load_event) => {
                load_event.send_empty().expect("send");
            }
            WriteEvent::Save(_) => unreachable!("must not happen"),
            WriteEvent::Stats(_) => {}
        }
    }
}
