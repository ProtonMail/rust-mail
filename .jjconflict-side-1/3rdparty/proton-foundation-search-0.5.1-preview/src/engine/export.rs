use std::fmt::Debug;
use std::sync::Arc;

use itertools::Itertools;

use crate::chunker::ChunkIter;
use crate::engine::{Engine, InnerEngine, MANIFEST, Manifest, Write};
use crate::entry::{Entry, EntryValue};
use crate::index::collection::{CollectionReadEvent, CollectionWriteOperation};
use crate::index::prelude::IndexExportEvent;
use crate::transaction::{LoadEvent, NoCache, TransactionState};

impl Engine {
    /// Create an engine export iterator.
    #[doc = include_str!("../../README.EXPORT.md")]
    pub fn export(&self) -> Export {
        Export {
            engine: self.inner.clone(),
            state: TransactionState::no_cache(MANIFEST.into(), Manifest::default),
            stage: Stage::Init,
        }
    }
}

#[derive(Debug)]
pub struct Export {
    engine: Arc<InnerEngine>,
    state: TransactionState<NoCache<Manifest>, Manifest>,
    stage: Stage,
}

/// Event returned from `engine.export()`
pub enum ExportEvent {
    /// A blob needs to be loaded.
    ///
    /// This even must be handled (`send()`) before calling `next()` again.
    Load(LoadEvent),
    /// An export entry item
    Entry(Entry),
}

enum Stage {
    Init,
    Collection(Box<dyn Send + Iterator<Item = CollectionReadEvent>>),
    Iterating(Box<dyn Send + Iterator<Item = ExportEvent>>),
}

impl Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init => write!(f, "Init"),
            Self::Collection(_) => f.debug_tuple("Collection").finish_non_exhaustive(),
            Self::Iterating(_) => f.debug_tuple("Dump").finish(),
        }
    }
}

impl Iterator for Export {
    type Item = ExportEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            engine,
            stage,
            state,
            ..
        } = self;

        let manifest = match state.load()? {
            Ok(manifest) => manifest,
            Err(load) => return Some(ExportEvent::Load(load)),
        };

        loop {
            break match stage {
                Stage::Init => {
                    *stage = Stage::Collection(Box::new(
                        engine.collection.read(manifest.collection_revision),
                    ));
                    continue;
                }
                Stage::Collection(collection) => match collection.next()? {
                    CollectionReadEvent::Load(load) => Some(ExportEvent::Load(load)),
                    CollectionReadEvent::Ready(collection) => {
                        let export = engine
                            .indices
                            .iter()
                            .map(|(id, index)| {
                                let revision = manifest
                                    .index_revisions
                                    .get(id)
                                    .copied()
                                    .unwrap_or_default();
                                index.export(revision)
                            })
                            .kmerge_by(|a, b| {
                                match (a, b) {
                                    // sorting is used for merging same entry-attr later
                                    (IndexExportEvent::Load(a), IndexExportEvent::Load(b)) => {
                                        a.name < b.name
                                    }
                                    (IndexExportEvent::Load(_), _) => true,
                                    (_, IndexExportEvent::Load(_)) => false,
                                    (
                                        IndexExportEvent::Entry {
                                            entry: entry_a,
                                            attr: attr_a,
                                            ..
                                        },
                                        IndexExportEvent::Entry {
                                            entry: entry_b,
                                            attr: attr_b,
                                            ..
                                        },
                                    ) => {
                                        entry_a < entry_b || (entry_a == entry_b && attr_a < attr_b)
                                    }
                                }
                            })
                            .chunk(
                                |event| match event {
                                    IndexExportEvent::Load(load_event) => {
                                        // do not chunk load events
                                        (0, 0, Some(load_event.name.clone()))
                                    }
                                    IndexExportEvent::Entry { entry, attr, .. } => {
                                        (entry.0, attr.0, None)
                                    }
                                },
                                |e| e,
                            )
                            .filter_map({
                                // merge entries together
                                |(_, mut chunk)| {
                                    let (entry, attr, mut result) = match chunk.next()? {
                                        IndexExportEvent::Entry { entry, attr, value } => {
                                            (entry, attr, value)
                                        }
                                        other => return Some(other),
                                    };
                                    for event in chunk {
                                        match event {
                                            IndexExportEvent::Entry { value, .. } => {
                                                result.resize(
                                                    result.len().max(value.len()),
                                                    EntryValue::Empty,
                                                );
                                                for (offset, v) in value.into_iter().enumerate() {
                                                    if v != EntryValue::Empty {
                                                        result[offset] = v;
                                                    }
                                                }
                                            }
                                            other => return Some(other),
                                        }
                                    }
                                    Some(IndexExportEvent::Entry {
                                        entry,
                                        attr,
                                        value: result,
                                    })
                                }
                            })
                            .map(move |event| match event {
                                IndexExportEvent::Load(load_event) => ExportEvent::Load(load_event),
                                IndexExportEvent::Entry { entry, attr, value } => {
                                    ExportEvent::Entry(Entry::new(
                                        collection.get_identifier(entry),
                                        [(
                                            collection.get_attribute_name(attr).into(),
                                            Arc::new(value),
                                        )]
                                        .into(),
                                    ))
                                }
                            });

                        *stage = Stage::Iterating(Box::new(export));
                        continue;
                    }
                },
                Stage::Iterating(dump) => dump.next(),
            };
        }
    }
}

impl Write {
    /// Prepares the worker for inserting a new import when committing.
    #[inline]
    #[tracing::instrument(skip_all)]
    pub fn import(&mut self, entry: Entry) {
        let (identifier, attributes) = entry.into();
        self.operations
            .push(CollectionWriteOperation::Insert(identifier, attributes));
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::RwLock;

    use arc_swap::ArcSwapOption;

    use super::*;
    use crate::entry::EntryValue;
    use crate::index::collection::{CollectionContent, CollectionSansIo};
    use crate::index::prelude::{Index, IndexExport, IndexSearch, IndexStore};
    use crate::processor::Processor;
    use crate::serialization::SerDes;

    #[test]
    fn exports() {
        // Entries are sorted
        let mut collection = CollectionContent::default();
        collection.insert_attribute("a".into());
        collection.insert_attribute("b".into());
        let _ = collection.insert_entry("x".into(), 0);
        let _ = collection.insert_entry("y".into(), 0);
        let collection = Arc::new(ArcSwapOption::new(Some(Arc::new((1, collection)))));
        let collection = CollectionSansIo::test_new(collection);

        let sut = Engine {
            inner: Arc::new(InnerEngine {
                collection,
                indices: [
                    fake(
                        "fake1",
                        [Some(IndexExportEvent::Entry {
                            entry: 1.into(),
                            attr: 0.into(),
                            value: vec![
                                true.into(),
                                false.into(),
                                1.into(),
                                2.into(),
                                3.into(),
                                "M".into(),
                                "N".into(),
                                ["f1", "f2"]
                                    .into_iter()
                                    .map(Into::into)
                                    .enumerate()
                                    .collect::<Vec<_>>()
                                    .into(),
                            ],
                        })],
                    ),
                    fake(
                        "fake2",
                        [Some(IndexExportEvent::Entry {
                            entry: 0.into(),
                            attr: 0.into(),
                            value: vec![
                                false.into(),
                                4.into(),
                                "O".into(),
                                ["f3"]
                                    .into_iter()
                                    .map(Into::into)
                                    .enumerate()
                                    .collect::<Vec<_>>()
                                    .into(),
                            ],
                        })],
                    ),
                ]
                .into(),
                processor: Box::new(Processor::default()),
                writer: false.into(),
                current_batch: 0.into(),
            }),
        };

        let export = sut.export();
        let mut entries = vec![];
        for event in export {
            match event {
                ExportEvent::Load(load_event) => {
                    // we would only load the manifest
                    assert!(load_event.name.starts_with(MANIFEST));
                    let manifest = SerDes::Cbor
                        .serialize(&(
                            0,
                            Manifest {
                                collection_revision: 1,
                                index_revisions: [("fake1".into(), 1), ("fake2".into(), 1)].into(),
                                active_blobs: [].into(),
                                released_blobs: [].into(),
                            },
                        ))
                        .expect("serialize");
                    (load_event.send)(&SerDes::Cbor, manifest).expect("send");
                }
                ExportEvent::Entry(entry) => entries.push(entry),
            }
        }

        insta::assert_debug_snapshot!(entries, @r#"
        [
            Entry {
                identifier: "x",
                attributes: {
                    "a": [
                        Boolean(
                            false,
                        ),
                        Integer(
                            4,
                        ),
                        Tag(
                            "O",
                        ),
                        Text(
                            [
                                (
                                    0,
                                    "f3",
                                ),
                            ],
                        ),
                    ],
                },
            },
            Entry {
                identifier: "y",
                attributes: {
                    "a": [
                        Boolean(
                            true,
                        ),
                        Boolean(
                            false,
                        ),
                        Integer(
                            1,
                        ),
                        Integer(
                            2,
                        ),
                        Integer(
                            3,
                        ),
                        Tag(
                            "M",
                        ),
                        Tag(
                            "N",
                        ),
                        Text(
                            [
                                (
                                    0,
                                    "f1",
                                ),
                                (
                                    1,
                                    "f2",
                                ),
                            ],
                        ),
                    ],
                },
            },
        ]
        "#);
    }

    #[test]
    fn merges_exports() {
        // The engine export shall merge subsequent entry attr values from different indices into one
        let mut collection = CollectionContent::default();
        collection.insert_attribute("a".into());
        collection.insert_attribute("b".into());
        let _ = collection.insert_entry("x".into(), 0);
        let _ = collection.insert_entry("y".into(), 0);
        let collection = Arc::new(ArcSwapOption::new(Some(Arc::new((1, collection)))));
        let collection = CollectionSansIo::test_new(collection);

        let sut = Engine {
            inner: Arc::new(InnerEngine {
                collection,
                indices: [
                    fake(
                        "fake1",
                        [Some(IndexExportEvent::Entry {
                            entry: 0.into(),
                            attr: 0.into(),
                            value: vec![
                                true.into(),
                                EntryValue::Empty,
                                3.into(),
                                EntryValue::Empty,
                                "5".into(),
                                EntryValue::Empty,
                                EntryValue::Empty,
                                ["f2", "f3"]
                                    .into_iter()
                                    .map(Into::into)
                                    .enumerate()
                                    .collect::<Vec<_>>()
                                    .into(),
                            ],
                        })],
                    ),
                    fake(
                        "fake2",
                        [Some(IndexExportEvent::Entry {
                            entry: 0.into(),
                            attr: 0.into(),
                            value: vec![
                                EntryValue::Empty,
                                2.into(),
                                EntryValue::Empty,
                                "4".into(),
                                EntryValue::Empty,
                                EntryValue::Empty,
                                ["f1"]
                                    .into_iter()
                                    .map(Into::into)
                                    .enumerate()
                                    .collect::<Vec<_>>()
                                    .into(),
                                EntryValue::Empty,
                                EntryValue::Empty,
                                false.into(),
                            ],
                        })],
                    ),
                ]
                .into(),
                processor: Box::new(Processor::default()),
                writer: false.into(),
                current_batch: 0.into(),
            }),
        };

        let export = sut.export();
        let mut entries = vec![];
        for event in export {
            match event {
                ExportEvent::Load(load_event) => {
                    // we would only load the manifest
                    assert!(load_event.name.starts_with(MANIFEST));
                    let manifest = SerDes::Cbor
                        .serialize(&(
                            0,
                            Manifest {
                                collection_revision: 1,
                                index_revisions: [("fake1".into(), 1), ("fake2".into(), 1)].into(),
                                active_blobs: [].into(),
                                released_blobs: [].into(),
                            },
                        ))
                        .expect("serialize");
                    (load_event.send)(&SerDes::Cbor, manifest).expect("send");
                }
                ExportEvent::Entry(entry) => entries.push(entry),
            }
        }

        insta::assert_debug_snapshot!(entries, @r#"
        [
            Entry {
                identifier: "x",
                attributes: {
                    "a": [
                        Boolean(
                            true,
                        ),
                        Integer(
                            2,
                        ),
                        Integer(
                            3,
                        ),
                        Tag(
                            "4",
                        ),
                        Tag(
                            "5",
                        ),
                        Empty,
                        Text(
                            [
                                (
                                    0,
                                    "f1",
                                ),
                            ],
                        ),
                        Text(
                            [
                                (
                                    0,
                                    "f2",
                                ),
                                (
                                    1,
                                    "f3",
                                ),
                            ],
                        ),
                        Empty,
                        Boolean(
                            false,
                        ),
                    ],
                },
            },
        ]
        "#);
    }

    fn fake(
        name: &'static str,
        events: impl IntoIterator<Item = Option<IndexExportEvent>>,
    ) -> (Box<str>, Box<dyn Index>) {
        (
            name.into(),
            Box::new((name, Arc::new(RwLock::new(events.into_iter().collect())))),
        )
    }
    type FakeIndex = (
        &'static str,
        Arc<RwLock<VecDeque<Option<IndexExportEvent>>>>,
    );
    impl IndexExport for FakeIndex {
        fn export(
            &self,
            _revision: u64,
        ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
            let (_, fake) = self.clone();
            Box::new(std::iter::from_fn(move || {
                RwLock::write(&fake)
                    .expect("fake pop")
                    .pop_front()
                    .flatten()
            }))
        }
    }
    impl IndexSearch for FakeIndex {
        fn search(
            &self,
            _revision: u64,
            _attribute: Option<crate::index::prelude::AttributeIndex>,
            _function: crate::query::expression::Func,
            _value: &crate::document::Value,
            _options: &crate::query::option::QueryOptions,
        ) -> Option<
            Box<dyn 'static + Send + Iterator<Item = crate::index::prelude::IndexSearchEvent>>,
        > {
            unimplemented!()
        }
    }
    impl IndexStore for FakeIndex {
        fn id(&self) -> &str {
            self.0
        }

        fn write(
            &self,
            _revision: u64,
            _operations: &[crate::index::prelude::IndexStoreOperation],
        ) -> Box<dyn Send + Iterator<Item = crate::index::prelude::IndexStoreEvent>> {
            unimplemented!()
        }

        fn reset(&self) {
            unimplemented!()
        }
    }

    #[test]
    fn structure() {
        let entry = Entry::new(
            "entry1",
            [
                ("created".into(), Arc::new(vec![93939393.into()])),
                ("category".into(), Arc::new(vec!["story:fantasy".into()])),
                (
                    "mixung".into(),
                    Arc::new(vec![
                        vec![(0, "dwarf".into()), (6, "fairy".into())].into(),
                        "mixtag".into(),
                        3.into(),
                    ]),
                ),
            ]
            .into(),
        );
        let json = serde_json::to_string_pretty(&entry).expect("serialize");
        insta::assert_snapshot!(json, @r#"
        {
          "identifier": "entry1",
          "attributes": {
            "category": [
              "story:fantasy"
            ],
            "created": [
              93939393
            ],
            "mixung": [
              [
                [
                  0,
                  "dwarf"
                ],
                [
                  6,
                  "fairy"
                ]
              ],
              "mixtag",
              3
            ]
          }
        }
        "#);

        insta::assert_debug_snapshot!(entry, @r#"
        Entry {
            identifier: "entry1",
            attributes: {
                "category": [
                    Tag(
                        "story:fantasy",
                    ),
                ],
                "created": [
                    Integer(
                        93939393,
                    ),
                ],
                "mixung": [
                    Text(
                        [
                            (
                                0,
                                "dwarf",
                            ),
                            (
                                6,
                                "fairy",
                            ),
                        ],
                    ),
                    Tag(
                        "mixtag",
                    ),
                    Integer(
                        3,
                    ),
                ],
            },
        }
        "#);
    }

    #[test]
    fn future_proofing() {
        use std::collections::BTreeMap;

        use serde::{Deserialize, Serialize};

        #[derive(Debug, Serialize, Deserialize)]
        struct FutureProofEntry {
            identifier: Box<str>,
            attributes: BTreeMap<Box<str>, Vec<FutureProofValue>>,
        }

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(untagged)]
        enum FutureProofValue {
            Known(EntryValue),
            Future(serde_json::Value),
        }

        impl From<FutureProofEntry> for Entry {
            fn from(value: FutureProofEntry) -> Self {
                Entry::new(
                    value.identifier,
                    value
                        .attributes
                        .into_iter()
                        .map(|(k, v)| (k, Arc::new(v.into_iter().map(Into::into).collect())))
                        .collect(),
                )
            }
        }

        impl From<FutureProofValue> for EntryValue {
            fn from(value: FutureProofValue) -> Self {
                match value {
                    FutureProofValue::Known(value) => value,
                    FutureProofValue::Future(value) => {
                        tracing::warn!("Ignoring unknown value on import {value:#?}");
                        EntryValue::Empty
                    }
                }
            }
        }

        // let's imagine a future version of the app will add a new type of a value
        // that the current engine doesn't know about. It could be a geo coordinate for instance:
        // the `[123.123, 321.321, 111.4]` in the "futured" attribute
        let json = r#"
        {
        "identifier": "entry1",
        "attributes": {
            "alright": [
            "mixtag",
            3
            ],
            "futured": [
                [123.123, 321.321, 111.4]
            ]
        }
        }
        "#;

        let entry: FutureProofEntry = serde_json::from_str(json).expect("deserialied");

        // note that the unknown value is deserialized as a json value
        insta::assert_debug_snapshot!(entry, @r#"
        FutureProofEntry {
            identifier: "entry1",
            attributes: {
                "alright": [
                    Known(
                        Tag(
                            "mixtag",
                        ),
                    ),
                    Known(
                        Integer(
                            3,
                        ),
                    ),
                ],
                "futured": [
                    Future(
                        Array [
                            Number(123.123),
                            Number(321.321),
                            Number(111.4),
                        ],
                    ),
                ],
            },
        }
        "#);

        let entry_for_import: Entry = entry.into();

        // note that the unknown value is replaced with `Empty`
        insta::assert_debug_snapshot!(entry_for_import, @r#"
        Entry {
            identifier: "entry1",
            attributes: {
                "alright": [
                    Tag(
                        "mixtag",
                    ),
                    Integer(
                        3,
                    ),
                ],
                "futured": [
                    Empty,
                ],
            },
        }
        "#);

        // The `entry_for_import` can now be imported into the engine:
        let engine = Engine::builder().build();
        let mut write = engine.write().expect("writer");
        write.import(entry_for_import);
        for _event in write.commit() {
            //todo
        }
    }
}
