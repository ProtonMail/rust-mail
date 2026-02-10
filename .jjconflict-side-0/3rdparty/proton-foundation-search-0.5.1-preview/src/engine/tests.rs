#![allow(clippy::expect_used)]
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::iter::from_fn;
use std::sync::RwLock;

use test_log::test;
use tracing::info;

use super::*;
use crate::document::{Document, Value};
use crate::index::prelude::*;
use crate::query::expression::Func;
use crate::query::option::QueryOptions;
use crate::serialization::SerDes;
use crate::transaction::{LoadEvent, SaveEvent};

#[derive(Debug)]
enum FakeStoreEvent {
    Load(&'static str),
    Save(&'static str),
    #[allow(dead_code)]
    Removed,
    Inserted,
}

// Type alias to reduce complexity
type CacheEntry = (u64, BTreeSet<EntryIndex>);

#[derive(Debug)]
#[allow(dead_code)]
enum FakeSearchEvent {
    Load(&'static str),
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
        revision: u64,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        _value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        Some(Box::new(FakeIter(
            self.reads
                .iter()
                .map(Option::as_ref)
                .map(|e| {
                    e.map(|e| match e {
                        FakeSearchEvent::Load(name) => IndexSearchEvent::Load(LoadEvent {
                            name: format!("{name} r{revision}").into_boxed_str(),
                            send: Box::new(|_, _| Ok(())),
                        }),
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
        _revision: u64,
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
        revision: u64,
        _operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        Box::new(FakeIter(
            self.writes
                .iter()
                .map(Option::as_ref)
                .map(|e| {
                    e.map(|e| match e {
                        FakeStoreEvent::Load(name) => IndexStoreEvent::Load(LoadEvent {
                            name: format!("{name} r{revision}").into_boxed_str(),
                            send: Box::new(|_, _| Ok(())),
                        }),
                        FakeStoreEvent::Save(name) => IndexStoreEvent::Save(SaveEvent {
                            name: format!("{name} r{revision}", revision = revision + 1)
                                .into_boxed_str(),
                            recv: Box::new(|_| Ok(vec![])),
                        }),
                        FakeStoreEvent::Removed => IndexStoreEvent::Removed {
                            entry: EntryIndex(0),
                            attr: AttributeIndex(0),
                            value: vec![0.into()],
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
    fn reset(&self) {
        unimplemented!()
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
    let mut storage: BTreeMap<Box<str>, Vec<u8>> = BTreeMap::new();

    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
        .with_index(FakeIndex {
            reads: vec![],
            writes: vec![
                Some(FakeStoreEvent::Load("fake")),
                Some(FakeStoreEvent::Inserted),
                Some(FakeStoreEvent::Save("deep fake")),
                Some(FakeStoreEvent::Save("fake")),
            ],
        })
        .build();

    let mut write = sut.write().expect("one writer");
    write
        .insert(Document::new("xyz").with_attribute("pkey", 123))
        .expect("insert");
    let write = write.commit();

    let results = write
        .map(|e| match e {
            WriteEvent::Modified(_) => "modified".into(),
            WriteEvent::Load(LoadEvent { name, send }) => {
                send(
                    &SerDes::Json,
                    storage.get(&name).cloned().unwrap_or_default(),
                )
                .expect("send");
                format!("load {name:?}")
            }
            WriteEvent::Save(SaveEvent { name, recv }) => {
                let data = recv(&SerDes::Json);

                if let Ok(ref data) = data {
                    storage.insert(name.clone(), data.clone());
                }
                format!(
                    "save {name:?} {}",
                    data.map(|v| String::from_utf8(v)
                        .map(|s| format!("{s:?}"))
                        .unwrap_or_else(|e| format!("{:?}", e.into_bytes())))
                        .unwrap_or_else(|e| format!("Error: {e}"))
                )
            }
        })
        .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(results, @r#"
    [
        "load \"manifest r0\"",
        "load \"collection r0\"",
        "save \"collection r1\" \"[1,{\\\"attributes\\\":[\\\"pkey\\\"],\\\"entries\\\":{\\\"xyz\\\":0},\\\"identifiers\\\":{\\\"0\\\":\\\"xyz\\\"}}]\"",
        "load \"fake r0\"",
        "modified",
        "save \"deep fake r1\" \"\"",
        "save \"fake r1\" \"\"",
        "save \"manifest r0\" \"[0,{\\\"collection_revision\\\":1,\\\"index_revisions\\\":{\\\"fake\\\":1},\\\"active_blobs\\\":[\\\"collection r1\\\",\\\"deep fake r1\\\",\\\"fake r1\\\"],\\\"released_blobs\\\":[\\\"collection r0\\\"]}]\"",
    ]
    "#);
}

#[test]
fn stops_when_not_loaded() {
    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
        .with_index(FakeIndex {
            reads: vec![],
            writes: vec![
                Some(FakeStoreEvent::Load("fake")),
                Some(FakeStoreEvent::Inserted),
                Some(FakeStoreEvent::Save("deep fake")),
                Some(FakeStoreEvent::Save("fake")),
            ],
        })
        .build();

    let mut write = sut.write().expect("single writer").commit();

    let results = (&mut write)
        .map(|e| match e {
            WriteEvent::Modified(_) => "modified".into(),
            WriteEvent::Load(LoadEvent { name, send: _ }) => {
                // not loading here
                format!("load {name:?}")
            }
            WriteEvent::Save(SaveEvent { name, recv }) => {
                format!(
                    "save {name:?} {}",
                    recv(&SerDes::Json)
                        .map(|v| String::from_utf8(v)
                            .map(|s| format!("{s:?}"))
                            .unwrap_or_else(|e| format!("{:?}", e.into_bytes())))
                        .unwrap_or_else(|e| format!("Error: {e}"))
                )
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(results, vec!["load \"manifest r0\"".to_owned()]);

    assert!(write.next().is_none(), "It is done when it's done.")
}

#[test]
fn concurrent_read_write_transactions() {
    let mut storage: BTreeMap<Box<str>, String> = BTreeMap::new();

    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
        .with_index(IdentityIndex::default())
        .build();

    // initial write to lift revisions
    let mut write = sut.write().expect("single writer");
    write
        .insert(Document::new("abc").with_attribute("pkey", 42))
        .expect("insert");
    write
        .insert(Document::new("bug").with_attribute("pkey", 24))
        .expect("insert bug");
    let write = write.commit();
    for event in write {
        info!(?event, message = "writes");
        match event {
            WriteEvent::Modified(_) => {}
            WriteEvent::Load(LoadEvent { name, send }) => send(
                &SerDes::Json,
                storage.get(&name).cloned().unwrap_or_default().into(),
            )
            .expect("send"),
            WriteEvent::Save(SaveEvent { name, recv }) => {
                storage.insert(
                    name,
                    String::from_utf8(recv(&SerDes::Json).expect("recv")).expect("str"),
                );
            }
        }
    }

    insta::assert_debug_snapshot!(storage, @r#"
    {
        "collection r1": "[1,{\"attributes\":[\"pkey\"],\"entries\":{\"abc\":0,\"bug\":1},\"identifiers\":{\"0\":\"abc\",\"1\":\"bug\"}}]",
        "manifest r0": "[0,{\"collection_revision\":1,\"index_revisions\":{\"identity\":1},\"active_blobs\":[\"collection r1\",\"mock r1\"],\"released_blobs\":[\"collection r0\"]}]",
        "mock r1": "[1,[0,1]]",
    }
    "#);

    // now we interleave a write with query
    let mut write = sut.write().expect("single writer");
    write.remove("bug");
    let mut write = write.commit();
    match write.next().expect("next") {
        WriteEvent::Load(LoadEvent { name, send }) if name.as_ref() == "manifest r0" => send(
            &SerDes::Json,
            storage.get(&name).cloned().unwrap_or_default().into(),
        )
        .expect("send"),
        otherwise => panic!("expected manifest load, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Save(SaveEvent { name, recv }) if name.as_ref() == "collection r2" => {
            storage.insert(
                name,
                String::from_utf8(recv(&SerDes::Json).expect("recv")).expect("str"),
            );
        }
        otherwise => panic!("expected collection save, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Load(LoadEvent { name, send }) if name.as_ref() == "mock r1" => send(
            &SerDes::Json,
            storage.get(&name).cloned().unwrap_or_default().into(),
        )
        .expect("send"),
        otherwise => panic!("expected manifest load, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Modified(id) if id.as_ref() == "bug" => {}
        otherwise => panic!("expected modifies, got {otherwise:?}"),
    }
    match write.next().expect("next") {
        WriteEvent::Save(SaveEvent { name, recv }) if name.as_ref() == "mock r2" => {
            storage.insert(
                name,
                String::from_utf8(recv(&SerDes::Json).expect("recv")).expect("str"),
            );
        }
        otherwise => panic!("expected fake save, got {otherwise:?}"),
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
            QueryEvent::Load(LoadEvent { name, send }) => {
                send(
                    &SerDes::Json,
                    storage.get(&name).cloned().unwrap_or_default().into(),
                )
                .expect("send");
                record.push(format!("loaded {name}"));
            }
            QueryEvent::Found(found) => record.push(format!("found {:?}", found.identifier())),
            QueryEvent::Stats(_) => record.push("stats".to_string()),
        }
    }

    // the query is not affected by uncommitted write transaction
    insta::assert_debug_snapshot!(record, @r#"
        [
            "loaded manifest r0",
            "loaded collection r1",
            "found \"abc\"",
        ]
        "#);

    // then we complete the write transaction
    match write.next().expect("next") {
        WriteEvent::Save(SaveEvent { name, recv }) if name.as_ref() == "manifest r0" => {
            storage.insert(
                name,
                String::from_utf8(recv(&SerDes::Json).expect("recv")).expect("str"),
            );
        }
        otherwise => panic!("expected manifest save, got {otherwise:?}"),
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
            QueryEvent::Load(LoadEvent { name, send }) => {
                send(
                    &SerDes::Json,
                    storage.get(&name).cloned().unwrap_or_default().into(),
                )
                .expect("send");
                record.push(format!("loaded {name}"));
            }
            QueryEvent::Found(found) => record.push(format!("found {:?}", found.identifier())),
            QueryEvent::Stats(_) => record.push("stats".to_string()),
        }
    }

    // the query is not affected by uncommitted write transaction
    insta::assert_debug_snapshot!(record, @r#"
        [
            "loaded manifest r0",
            "found \"abc\"",
        ]
        "#);
}

/// A minimal index implementation, just recording EntryIds
#[derive(Debug, Default)]
struct IdentityIndex {
    /// Note that here, read and write cache are one.
    /// This would probably lead to contention or race conditions in a busy environment.
    /// So this is good enough for a test case only.
    entries: Arc<RwLock<Option<CacheEntry>>>,
}
impl IndexStore for IdentityIndex {
    fn id(&self) -> &str {
        "identity"
    }
    fn write(
        &self,
        revision: u64,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        let entries = self.entries.clone();
        let mut ops = Some(operations.iter().cloned().collect::<VecDeque<_>>());
        Box::new(from_fn(move || {
            let mut entries_opt = entries.try_write().expect("write lock");
            let entries2 = entries_opt.as_mut();
            let Some(entries3) = entries2.filter(|(r, _)| *r == revision) else {
                let entries = entries.clone();
                return Some(IndexStoreEvent::Load(LoadEvent {
                    name: format!("mock r{revision}").into(),
                    send: Box::new(move |serdes, data| {
                        let mut entries = entries.try_write().expect("write lock in Load");
                        *entries = Some(if data.is_empty() {
                            Default::default()
                        } else {
                            serdes.deserialize(&data)?
                        });
                        Ok(())
                    }),
                }));
            };
            let Some(op) = ops.as_mut()?.pop_front() else {
                ops = None;
                let entries = entries.clone();
                let revision = revision.wrapping_add(1);
                return Some(IndexStoreEvent::Save(SaveEvent {
                    name: format!("mock r{revision}").into(),
                    recv: Box::new(move |serdes| {
                        let entries = entries.read().expect("read lock in Save");
                        match entries.as_ref() {
                            Some((_, entries)) => Ok(serdes.serialize(&(revision, entries))?),
                            None => Ok(vec![]),
                        }
                    }),
                }));
            };
            match op {
                IndexStoreOperation::Insert(entry, attr, ..) => {
                    entries3.1.insert(entry);
                    Some(IndexStoreEvent::Inserted { entry, attr })
                }
                IndexStoreOperation::Remove(entry) => {
                    entries3
                        .1
                        .remove(&entry)
                        .then_some(IndexStoreEvent::Removed {
                            entry,
                            attr: AttributeIndex(0),
                            value: EntryValues::default(),
                        })
                }
            }
        }))
    }
    fn reset(&self) {
        unimplemented!()
    }
}
impl IndexExport for IdentityIndex {
    fn export(
        &self,
        _revision: u64,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        unimplemented!()
    }
}
impl IndexSearch for IdentityIndex {
    fn search(
        &self,
        revision: u64,
        _attribute: Option<AttributeIndex>,
        _function: Func,
        _value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        let entries = self.entries.clone();
        let mut ops = None;
        Some(Box::new(from_fn(move || {
            let entries_opt = entries.try_read().expect("write lock");
            let entries2 = entries_opt.as_ref();
            let Some(entries3) = entries2 else {
                let entries = entries.clone();
                return Some(IndexSearchEvent::Load(LoadEvent {
                    name: format!("mock r{revision}").into(),
                    send: Box::new(move |serdes, data| {
                        let mut entries = entries.try_write().expect("write lock in Load");
                        *entries = Some(serdes.deserialize(&data)?);
                        Ok(())
                    }),
                }));
            };
            let ops =
                ops.get_or_insert_with(|| entries3.1.iter().cloned().collect::<VecDeque<_>>());

            ops.pop_front().map(|e| IndexSearchEvent::Found(e, vec![]))
        })))
    }
}

#[test]
fn concurrent_writes_not_allowed() {
    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
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
        .with_builtin_processor(Default::default())
        .with_default_indices();
    assert!(sut.with_text_index().is_err());

    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
        .with_boolean_index();
    let sut = sut.with_text_index().expect("another index");
    assert!(sut.with_boolean_index().is_err());

    let sut = Engine::builder()
        .with_builtin_processor(Default::default())
        .with_index(FakeIndex::default());
    assert!(sut.with_index(FakeIndex::default()).is_err());
}
