use std::collections::{BTreeMap, VecDeque};
use std::iter::once;

use super::*;
use crate::index::prelude::*;
use crate::transaction::{LoadEvent, ReleaseEvent, SaveEvent, TransactionState, Write};

#[derive(Debug)]
pub enum CollectionStoreEvent {
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
        revision: u64,
        operations: &[CollectionWriteOperation],
        batch_number: u32,
    ) -> Writer {
        Writer::new(self, revision, operations, batch_number)
    }
}

#[derive(Debug)]
pub struct Writer {
    state: Option<TransactionState<Write<CollectionContent>, CollectionContent>>,
    modified: bool,
    operations: VecDeque<Operation>,
    /// obsolete blob names to release
    released_blobs: VecDeque<ReleaseEvent>,
    /// current batch number for EntryIndex generation
    batch_number: u32,
}

impl Writer {
    fn new(
        collection: &CollectionSansIo,
        revision: u64,
        operations: &[CollectionWriteOperation],
        batch_number: u32,
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

        let state = Some(TransactionState::write(
            revision,
            COLLECTION.into(),
            collection.reader.load_full(),
            collection.writer.clone(),
        ));

        Self {
            operations,
            state,
            modified: false,
            released_blobs: [].into(),
            batch_number,
        }
    }
}

impl Iterator for Writer {
    type Item = CollectionStoreEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(released) = self.released_blobs.pop_front() {
            return Some(CollectionStoreEvent::Release(released));
        }

        let collection = match self.state.as_mut()?.load()? {
            Ok(cached) => cached,
            Err(load) => return Some(CollectionStoreEvent::Load(load)),
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
                        let entry =
                            match collection.insert_entry(identifier.clone(), self.batch_number) {
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
            } else if let (true, Some(state)) = (
                std::mem::take(&mut self.modified),
                std::mem::take(&mut self.state),
            ) {
                let (save, release) = state.save();
                if let Some(release) = release {
                    self.released_blobs.push_back(release);
                }
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

#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use test_log::test;

    use crate::index::collection::{
        CollectionSansIo, CollectionStoreEvent, CollectionWriteOperation,
    };
    use crate::index::prelude::*;
    use crate::serialization::SerDes;
    use crate::transaction::{LoadEvent, ReleaseEvent, SaveEvent};

    #[test]
    fn insert() {
        let mut blobs = BTreeMap::new();
        let sut = CollectionSansIo::default();

        for round in 0..2 {
            // act - insert some content
            // do it twice so we test blob release as well
            let write = sut.write(
                round,
                &[CollectionWriteOperation::Insert(
                    "abc".into(),
                    [(
                        format!("attr{round}").into_boxed_str(),
                        Arc::new(vec![1.into()]),
                    )]
                    .into(),
                )],
                0,
            );
            for event in write {
                match event {
                    CollectionStoreEvent::Release(ReleaseEvent { name }) => {
                        blobs
                            .entry(name)
                            .and_modify(|data| {
                                *data = b"released: ".iter().chain(&*data).copied().collect()
                            })
                            .or_insert(b"released.".into());
                    }
                    CollectionStoreEvent::Load(LoadEvent { send, .. }) => {
                        (send)(&crate::serialization::SerDes::Cbor, vec![]).expect("send")
                    }
                    CollectionStoreEvent::Save(SaveEvent { name, recv }) => {
                        blobs.insert(name, (recv)(&SerDes::Json).expect("recv"));
                    }
                    _ => {}
                }
            }
        }

        // assert blobs - one released one up to date
        insta::assert_debug_snapshot!(blobs.into_iter().map(|(k,v)|(k,String::from_utf8(v).expect("utf-8"))).collect::<Vec<_>>(), @r#"
        [
            (
                "collection r0",
                "released.",
            ),
            (
                "collection r1",
                "released: [1,{\"attributes\":[\"attr0\"],\"entries\":{\"abc\":0},\"identifiers\":{\"0\":\"abc\"}}]",
            ),
            (
                "collection r2",
                "[2,{\"attributes\":[\"attr0\",\"attr1\"],\"entries\":{\"abc\":0},\"identifiers\":{\"0\":\"abc\"}}]",
            ),
        ]
        "#);
        // assert that the collection has content
        insta::assert_debug_snapshot!(sut, @r#"
        CollectionSansIo {
            reader: ArcSwapAny(
                None,
            ),
            writer: ArcSwapAny(
                Some(
                    (
                        2,
                        CollectionContent {
                            attributes: {
                                "attr0",
                                "attr1",
                            },
                            entries: {
                                "abc": EntryIndex(
                                    0,
                                ),
                            },
                            identifiers: {
                                EntryIndex(
                                    0,
                                ): "abc",
                            },
                        },
                    ),
                ),
            ),
        }
        "#);
    }

    #[test]
    fn insert_collection_rev0() {
        let coll = CollectionSansIo::default();

        let mut write = coll.write(
            0,
            &[CollectionWriteOperation::Insert(
                "doc1".into(),
                [("attr1".into(), Arc::new(EntryValues::default()))].into(),
            )],
            0,
        );

        match write.next() {
            Some(CollectionStoreEvent::Load(LoadEvent { name, send }))
                if name.as_ref() == "collection r0" =>
            {
                send(&SerDes::Json, vec![]).expect("send")
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
            Some(CollectionStoreEvent::Attribute {
                attribute: AttributeIndex(0),
                name,
            }) if name.as_ref() == "attr1" => {}
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Save(SaveEvent { name, recv }))
                if name.as_ref() == "collection r1" =>
            {
                insta::assert_debug_snapshot!(recv(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "[1,{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}]",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Release(ReleaseEvent { name }))
                if name.as_ref() == "collection r0" => {}
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

        let mut write = coll.write(4, &[CollectionWriteOperation::Remove("doc1".into())], 0);

        match write.next() {
            Some(CollectionStoreEvent::Load(LoadEvent { name, send }))
                if name.as_ref() == "collection r4" =>
            {
                send(&SerDes::Json, b"[4,{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}]".to_vec())
                    .expect("send")
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
            Some(CollectionStoreEvent::Save(SaveEvent { name, recv }))
                if name.as_ref() == "collection r5" =>
            {
                insta::assert_debug_snapshot!(recv(&SerDes::Json).map(String::from_utf8), @r#"
                Ok(
                    Ok(
                        "[5,{\"attributes\":[\"attr1\"],\"entries\":{},\"identifiers\":{}}]",
                    ),
                )
                "#);
            }
            next => panic!("unexpected {next:?}"),
        }

        match write.next() {
            Some(CollectionStoreEvent::Release(ReleaseEvent { name }))
                if name.as_ref() == "collection r4" => {}
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
            4,
            &[CollectionWriteOperation::Remove("absent!!!".into())],
            0,
        );

        match write.next() {
            Some(CollectionStoreEvent::Load(LoadEvent { name, send }))
                if name.as_ref() == "collection r4" =>
            {
                send(&SerDes::Json, b"[4,{\"attributes\":[\"attr1\"],\"entries\":{\"doc1\":0},\"identifiers\":{\"0\":\"doc1\"}}]".to_vec())
                    .expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        // Removing absent entry makes no difference
        match write.next() {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }
}
