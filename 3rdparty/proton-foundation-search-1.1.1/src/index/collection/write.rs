use std::collections::{BTreeMap, VecDeque};
use std::iter::once;
use std::ops::ControlFlow;

use super::*;
use crate::blob::{Blob, BlobId, LoadEvent, ReleaseEvent, SaveEvent};
use crate::engine::Stats;
use crate::index::prelude::*;

#[derive(Debug)]
pub enum CollectionStoreEvent {
    /// Collection statistics
    Stats(Stats),
    /// An entry identifier has been inserted/found/removed
    Entry {
        /// inserted/found entry ID
        entry: EntryIndex,
        /// inserted identifier
        identifier: Box<str>,
    },
    /// An entry identifier has been inserted/found
    Attribute {
        /// inserted/found attribute
        attribute: AttributeIndex,
        /// inserted attribute name
        name: Box<str>,
    },
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
    /// The index store requests storage release
    Release(ReleaseEvent),
}

#[derive(Debug)]
pub enum CollectionWriteOperation {
    Insert(Box<str>, BTreeMap<Box<str>, Arc<EntryValues>>),
    Remove(Box<str>),
}

impl CollectionSansIo {
    pub fn write(
        &self,
        revision: Option<BlobId>,
        operations: &[CollectionWriteOperation],
    ) -> Writer {
        Writer::new(self, revision, operations)
    }
}

#[derive(Debug)]
pub struct Writer {
    blob: Option<Blob<CollectionContent>>,
    content: Option<CollectionContent>,
    modified: bool,
    operations: VecDeque<Operation>,
    events: VecDeque<CollectionStoreEvent>,
}

impl Writer {
    fn new(
        _collection: &CollectionSansIo,
        revision: Option<BlobId>,
        operations: &[CollectionWriteOperation],
    ) -> Self {
        let operations = operations
            .iter()
            .flat_map(|op| match op {
                CollectionWriteOperation::Remove(identifier) => {
                    vec![Operation::RemoveEntry(identifier.clone())]
                }
                CollectionWriteOperation::Insert(identifier, values) => {
                    once(Operation::InsertEntry(identifier.clone()))
                        .chain(
                            values
                                .keys()
                                .map(|field| Operation::InsertAttribute(field.clone())),
                        )
                        .collect()
                }
            })
            .collect();

        Self {
            operations,
            blob: revision.map(|revision| Blob::new(revision, COLLECTION)),
            content: None,
            modified: false,
            events: [].into(),
        }
    }
}

impl Iterator for Writer {
    type Item = CollectionStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.events.pop_front() {
            return Some(event);
        }

        let collection = match &mut self.content {
            Some(content) => content,
            None => match self.blob.as_mut().map(Blob::fetch_and_free) {
                Some(ControlFlow::Break(load)) => {
                    return Some(CollectionStoreEvent::Load(load));
                }
                Some(ControlFlow::Continue(loaded)) => self.content.insert(loaded.as_ref().clone()),
                None => self.content.insert(Default::default()),
            },
        };

        loop {
            break if let Some(op) = self.operations.pop_front() {
                Some(match op {
                    Operation::RemoveEntry(identifier) => {
                        let Some(entry) = collection.remove_entry(&identifier) else {
                            continue;
                        };
                        self.modified = true;
                        CollectionStoreEvent::Entry { entry, identifier }
                    }
                    Operation::InsertEntry(identifier) => {
                        let entry = match collection.insert_entry(identifier.clone()) {
                            Ok(entry) => {
                                self.modified = true;
                                entry
                            }
                            Err(entry) => entry,
                        };
                        CollectionStoreEvent::Entry { entry, identifier }
                    }
                    Operation::InsertAttribute(name) => {
                        self.modified |= collection.get_attribute(&name).is_none();
                        CollectionStoreEvent::Attribute {
                            attribute: collection.insert_attribute(name.clone()),
                            name,
                        }
                    }
                })
            } else if std::mem::take(&mut self.modified) {
                let collection = std::mem::take(collection);
                self.events.push_back(CollectionStoreEvent::Stats(Stats {
                    documents: collection.len(),
                }));

                let id = BlobId::generate(&collection);
                let (release, save) = self
                    .blob
                    .get_or_insert_with(|| Blob::new(id, COLLECTION))
                    .save_and_free(id, Arc::new(collection));
                self.events
                    .extend(release.map(CollectionStoreEvent::Release));
                Some(CollectionStoreEvent::Save(save))
            } else {
                None
            };
        }
    }
}

#[derive(Debug)]
enum Operation {
    RemoveEntry(Box<str>),
    InsertEntry(Box<str>),
    InsertAttribute(Box<str>),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use test_log::test;

    use crate::blob::{BlobId, Cached};
    use crate::engine::Stats;
    use crate::index::collection::{
        CollectionContent, CollectionSansIo, CollectionStoreEvent, CollectionWriteOperation,
    };
    use crate::index::prelude::*;
    use crate::serialization::SerDes;

    #[test]
    fn insert() {
        let mut revision = None;
        let mut blobs = BTreeMap::new();
        let sut = CollectionSansIo::default();

        for round in 0..2 {
            // act - insert some content
            // do it twice so we test blob release as well
            let write = sut.write(
                revision,
                &[CollectionWriteOperation::Insert(
                    "abc".into(),
                    [(
                        format!("attr{round}").into_boxed_str(),
                        Arc::new(vec![Some(1.into())]),
                    )]
                    .into(),
                )],
            );
            for event in write {
                match event {
                    CollectionStoreEvent::Release(release) => {
                        blobs
                            .entry(release.id())
                            .and_modify(|data| {
                                *data = b"released: ".iter().chain(&*data).copied().collect()
                            })
                            .or_insert(b"released.".to_vec());
                    }
                    CollectionStoreEvent::Load(load) => {
                        if let Some(data) = blobs.get(&load.id()) {
                            load.send(&SerDes::Json, data).expect("send data");
                        } else {
                            load.send_empty().expect("send");
                        }
                    }
                    CollectionStoreEvent::Save(save) => {
                        revision = Some(save.id());
                        blobs.insert(
                            save.id(),
                            save.recv().serialize(&SerDes::Json).expect("recv"),
                        );
                    }
                    _ => {}
                }
            }
        }

        // assert blobs - one released one up to date
        insta::assert_debug_snapshot!(blobs.into_iter().map(|(k,v)|(k,String::from_utf8(v).expect("utf-8"))).collect::<Vec<_>>(), @r#"
        [
            (
                762A47C4D60B5750BF1179EEAFD67889,
                "released: {\"attributes\":[\"attr0\"],\"entries\":{\"abc\":0},\"identifiers\":{\"0\":\"abc\"}}",
            ),
            (
                97E4BA10FAC5554E913C305FF2E011EE,
                "{\"attributes\":[\"attr0\",\"attr1\"],\"entries\":{\"abc\":0},\"identifiers\":{\"0\":\"abc\"}}",
            ),
        ]
        "#);
    }

    #[test]
    fn insert_collection_rev0() {
        let coll = CollectionSansIo::default();

        let mut write = coll.write(
            None,
            &[CollectionWriteOperation::Insert(
                "doc1".into(),
                [("attr1".into(), Arc::new(EntryValues::default()))].into(),
            )],
        );

        match write.next() {
            Some(CollectionStoreEvent::Entry {
                entry: EntryIndex(0),
                identifier,
            }) if identifier.as_ref() == "doc1" => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Attribute {
                attribute: AttributeIndex(0),
                name,
            }) if name.as_ref() == "attr1" => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Save(save))
                if save.id().to_string() == "FEF85F0E19DB5C2AACBB3BC52E7ED65C" =>
            {
                insta::assert_debug_snapshot!(save.recv().serialize(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Stats(_)) => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn remove_collection_rev4() {
        let coll = CollectionSansIo::default();

        let mut write = coll.write(
            Some(BlobId::new(0, 4)),
            &[CollectionWriteOperation::Remove("doc1".into())],
        );

        match write.next() {
            Some(CollectionStoreEvent::Load(load)) if load.id() == BlobId::new(0, 4) => {
                load.send(&SerDes::Json, b"{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}")
                    .expect("send");
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Entry {
                entry: EntryIndex(0),
                identifier,
            }) if identifier.as_ref() == "doc1" => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Save(save))
                if save.id().to_string() == "566DC18FBEE151EC92825513AEEA9A2D" =>
            {
                insta::assert_debug_snapshot!(save.recv().serialize(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "{\"attributes\":[\"attr1\"],\"entries\":{},\"identifiers\":{}}",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Stats(_)) => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Release(release)) if release.id() == BlobId::new(0, 4) => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn unmodified() {
        let coll = CollectionSansIo::default();

        let mut write = coll.write(
            Some(BlobId::new(0, 4)),
            &[CollectionWriteOperation::Remove("absent!!!".into())],
        );

        match write.next() {
            Some(CollectionStoreEvent::Load(load)) if load.id() == BlobId::new(0, 4) => {
                load.send(&SerDes::Json, b"{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}")
                    .expect("send");
            }
            next => panic!("unexpected {next:?}"),
        }

        // Removing absent entry makes no difference
        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn fills_gaps() {
        let coll = CollectionSansIo::default();
        let mut cache = BTreeMap::new();
        let mut revision = None;
        let handler = |event,
                       cache: &mut BTreeMap<BlobId, Cached>,
                       stats: &mut Option<Stats>,
                       revision: &mut Option<BlobId>| match event {
            CollectionStoreEvent::Stats(s) => *stats = Some(s),
            CollectionStoreEvent::Entry { .. } | CollectionStoreEvent::Attribute { .. } => {}
            CollectionStoreEvent::Load(load_event) => {
                if let Some(cached) = cache.get(&load_event.id()) {
                    load_event.send_cached(cached).expect("msg");
                } else {
                    load_event.send_empty().expect("msg");
                }
            }
            CollectionStoreEvent::Save(save_event) => {
                *revision = Some(save_event.id());
                cache.insert(save_event.id(), save_event.recv());
            }
            CollectionStoreEvent::Release(release_event) => {
                cache.remove(&release_event.id());
            }
        };

        let mut stats = None;
        coll.write(
            revision,
            &(0..6)
                .map(|idx| {
                    CollectionWriteOperation::Insert(
                        format!("doc{idx}").into_boxed_str(),
                        [("a".into(), Arc::new(vec![Some(true.into())]))].into(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .for_each(|event| handler(event, &mut cache, &mut stats, &mut revision));
        assert_eq!(stats.clone().expect("stats").documents, 6);

        let mut stats = None;
        coll.write(
            revision,
            &[
                CollectionWriteOperation::Remove("doc0".into()),
                CollectionWriteOperation::Remove("doc1".into()),
                CollectionWriteOperation::Remove("doc3".into()),
                CollectionWriteOperation::Remove("doc4".into()),
            ],
        )
        .for_each(|event| handler(event, &mut cache, &mut stats, &mut revision));
        assert_eq!(stats.clone().expect("stats").documents, 2);

        let mut stats = None;
        coll.write(
            revision,
            &(0..6)
                .map(|idx| {
                    CollectionWriteOperation::Insert(
                        format!("replacement{idx}").into_boxed_str(),
                        [("a".into(), Arc::new(vec![Some(true.into())]))].into(),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .for_each(|event| handler(event, &mut cache, &mut stats, &mut revision));
        assert_eq!(stats.clone().expect("stats").documents, 8);

        let dump = cache
            .iter()
            .map(|(k, v)| {
                (
                    k,
                    v.downcast::<CollectionContent>("collection")
                        .expect("downcast"),
                )
            })
            .collect::<Vec<_>>();
        insta::assert_debug_snapshot!(dump, @r#"
        [
            (
                ACE29EB6207C5D25BC825C6E866B17E1,
                CollectionContent {
                    attributes: {
                        "a",
                    },
                    entries: {
                        "doc2": EntryIndex(
                            2,
                        ),
                        "doc5": EntryIndex(
                            5,
                        ),
                        "replacement0": EntryIndex(
                            0,
                        ),
                        "replacement1": EntryIndex(
                            1,
                        ),
                        "replacement2": EntryIndex(
                            3,
                        ),
                        "replacement3": EntryIndex(
                            4,
                        ),
                        "replacement4": EntryIndex(
                            6,
                        ),
                        "replacement5": EntryIndex(
                            7,
                        ),
                    },
                    identifiers: {
                        EntryIndex(
                            0,
                        ): "replacement0",
                        EntryIndex(
                            1,
                        ): "replacement1",
                        EntryIndex(
                            2,
                        ): "doc2",
                        EntryIndex(
                            3,
                        ): "replacement2",
                        EntryIndex(
                            4,
                        ): "replacement3",
                        EntryIndex(
                            5,
                        ): "doc5",
                        EntryIndex(
                            6,
                        ): "replacement4",
                        EntryIndex(
                            7,
                        ): "replacement5",
                    },
                },
            ),
        ]
        "#);
    }
}
