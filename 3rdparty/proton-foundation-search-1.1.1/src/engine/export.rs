use std::fmt::Debug;
use std::ops::ControlFlow;
use std::sync::Arc;

use itertools::Itertools;
use tracing::trace;

use crate::blob::{Blob, LoadEvent};
use crate::chunker::ChunkIter;
use crate::engine::{Engine, InnerEngine, MANIFEST, MANIFEST_BLOB_ID, Manifest};
use crate::entry::Entry;
use crate::index::collection::CollectionReadEvent;
use crate::index::prelude::IndexExportEvent;

impl Engine {
    /// Create an engine export iterator.
    #[doc = include_str!("../../README.EXPORT.md")]
    pub fn export(&self) -> Export {
        Export {
            engine: self.inner.clone(),
            blob: Blob::new(MANIFEST_BLOB_ID, MANIFEST),
            stage: Stage::Init,
        }
    }
}

#[derive(Debug)]
pub struct Export {
    engine: Arc<InnerEngine>,
    blob: Blob<Manifest>,
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
            blob,
            ..
        } = self;

        let manifest = match blob.load() {
            ControlFlow::Continue(manifest) => manifest,
            ControlFlow::Break(load) => return Some(ExportEvent::Load(load)),
        };

        loop {
            break match stage {
                Stage::Init => {
                    *stage = Stage::Collection(Box::new({
                        let coll = engine.collection.clone();
                        manifest
                            .collection_revision
                            .into_iter()
                            .flat_map(move |revision| coll.read(revision))
                    }));
                    continue;
                }
                Stage::Collection(collection) => match collection.next()? {
                    CollectionReadEvent::Load(load) => Some(ExportEvent::Load(load)),
                    CollectionReadEvent::Ready(collection) => {
                        let export = engine
                            .indices
                            .iter()
                            .filter_map(|(id, index)| {
                                let revision = *manifest.index_revisions.get(id)?;
                                Some(index.export(revision))
                            })
                            .kmerge_by(|a, b| {
                                match (a, b) {
                                    // sorting is used for merging same entry-attr later
                                    (IndexExportEvent::Load(a), IndexExportEvent::Load(b)) => {
                                        a.id() < b.id()
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
                                        (0, 0, Some(load_event.id()))
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
                                    trace!("iterating");

                                    let (entry, attr, mut result) = match chunk.next()? {
                                        IndexExportEvent::Entry { entry, attr, value } => {
                                            (entry, attr, value)
                                        }
                                        other => {
                                            trace!("passing {other:?}");
                                            return Some(other);
                                        }
                                    };
                                    for event in chunk {
                                        match event {
                                            IndexExportEvent::Entry { value, .. } => {
                                                result.resize(result.len().max(value.len()), None);
                                                for (offset, v) in value.into_iter().enumerate() {
                                                    if v.is_some() {
                                                        result[offset] = v;
                                                    }
                                                }
                                            }
                                            other => {
                                                trace!("passing {other:?}");
                                                return Some(other);
                                            }
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::RwLock;

    use test_log::test;

    use super::*;
    use crate::blob::{BlobId, Cached};
    use crate::entry::EntryValue;
    use crate::index::collection::{CollectionContent, CollectionSansIo};
    use crate::index::prelude::{Index, IndexExport, IndexSearch, IndexStore};
    use crate::processor::TextProcessor;

    #[test]
    fn exports() {
        // Entries are sorted
        let mut collection = CollectionContent::default();
        collection.insert_attribute("a".into());
        collection.insert_attribute("b".into());
        let _ = collection.insert_entry("x".into());
        let _ = collection.insert_entry("y".into());
        let collection_content = Arc::new(collection);
        let collection_blob_id = BlobId::new(0, 1);
        let collection = CollectionSansIo::default();

        let index1_blob_id = BlobId::new(1, 0);
        let index2_blob_id = BlobId::new(2, 0);

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
                                EntryValue::new(true),
                                EntryValue::new(false),
                                EntryValue::new(1),
                                EntryValue::new(2),
                                EntryValue::new(3),
                                EntryValue::new("M"),
                                EntryValue::new("N"),
                                EntryValue::text(["f1", "f2"]),
                            ],
                        })],
                    ),
                    fake(
                        "fake2",
                        [Some(IndexExportEvent::Entry {
                            entry: 0.into(),
                            attr: 0.into(),
                            value: vec![
                                EntryValue::new(false),
                                EntryValue::new(4),
                                EntryValue::new("O"),
                                EntryValue::text(["f3"]),
                            ],
                        })],
                    ),
                ]
                .into(),
                processor: Box::new(TextProcessor::default()),
                writer: false.into(),
            }),
        };

        let export = sut.export();
        let mut entries = vec![];
        for event in export {
            match event {
                ExportEvent::Load(load_event) if load_event.id() == MANIFEST_BLOB_ID => {
                    let manifest = Manifest {
                        collection_revision: Some(collection_blob_id),
                        index_revisions: [
                            ("fake1".into(), index1_blob_id),
                            ("fake2".into(), index2_blob_id),
                        ]
                        .into(),
                        active_blobs: [].into(),
                        released_blobs: [].into(),
                    };
                    load_event
                        .send_cached(&Cached::new(MANIFEST, Arc::new(manifest)))
                        .expect("send manifest cache");
                }
                ExportEvent::Load(load_event) if load_event.id() == collection_blob_id => {
                    load_event
                        .send_cached(&Cached::new("collection", collection_content.clone()))
                        .expect("send collection cache");
                }
                ExportEvent::Load(load_event) => {
                    panic!("unexpected load event: {load_event:?}");
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
                        Some(
                            Boolean(
                                false,
                            ),
                        ),
                        Some(
                            Integer(
                                4,
                            ),
                        ),
                        Some(
                            Tag(
                                "O",
                            ),
                        ),
                        Some(
                            Text(
                                [
                                    (
                                        0,
                                        "f3",
                                    ),
                                ],
                            ),
                        ),
                    ],
                },
            },
            Entry {
                identifier: "y",
                attributes: {
                    "a": [
                        Some(
                            Boolean(
                                true,
                            ),
                        ),
                        Some(
                            Boolean(
                                false,
                            ),
                        ),
                        Some(
                            Integer(
                                1,
                            ),
                        ),
                        Some(
                            Integer(
                                2,
                            ),
                        ),
                        Some(
                            Integer(
                                3,
                            ),
                        ),
                        Some(
                            Tag(
                                "M",
                            ),
                        ),
                        Some(
                            Tag(
                                "N",
                            ),
                        ),
                        Some(
                            Text(
                                [
                                    (
                                        0,
                                        "f1",
                                    ),
                                    (
                                        3,
                                        "f2",
                                    ),
                                ],
                            ),
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
        let _ = collection.insert_entry("x".into());
        let _ = collection.insert_entry("y".into());
        let collection_content = Arc::new(collection);
        let collection_blob_id = BlobId::new(0, 1);
        let collection = CollectionSansIo::default();

        let index1_blob_id = BlobId::new(1, 0);
        let index2_blob_id = BlobId::new(2, 0);

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
                                EntryValue::new(true),
                                None,
                                EntryValue::new(3),
                                None,
                                EntryValue::new("5"),
                                None,
                                None,
                                EntryValue::text(["f2", "f3"]),
                            ],
                        })],
                    ),
                    fake(
                        "fake2",
                        [Some(IndexExportEvent::Entry {
                            entry: 0.into(),
                            attr: 0.into(),
                            value: vec![
                                None,
                                EntryValue::new(2),
                                None,
                                EntryValue::new("4"),
                                None,
                                None,
                                EntryValue::text(["f1"]),
                                None,
                                None,
                                EntryValue::new(false),
                            ],
                        })],
                    ),
                ]
                .into(),
                processor: Box::new(TextProcessor::default()),
                writer: false.into(),
            }),
        };

        let export = sut.export();
        let mut entries = vec![];
        for event in export {
            match event {
                ExportEvent::Load(load_event) if load_event.id() == MANIFEST_BLOB_ID => {
                    let manifest = Manifest {
                        collection_revision: Some(collection_blob_id),
                        index_revisions: [
                            ("fake1".into(), index1_blob_id),
                            ("fake2".into(), index2_blob_id),
                        ]
                        .into(),
                        active_blobs: [].into(),
                        released_blobs: [].into(),
                    };
                    load_event
                        .send_cached(&Cached::new(MANIFEST, Arc::new(manifest)))
                        .expect("send cached manifest");
                }
                ExportEvent::Load(load_event) if load_event.id() == collection_blob_id => {
                    load_event
                        .send_cached(&Cached::new("collection", collection_content.clone()))
                        .expect("send cached collection");
                }
                ExportEvent::Load(load_event) => {
                    panic!("unexpected load event {load_event:?}");
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
                        Some(
                            Boolean(
                                true,
                            ),
                        ),
                        Some(
                            Integer(
                                2,
                            ),
                        ),
                        Some(
                            Integer(
                                3,
                            ),
                        ),
                        Some(
                            Tag(
                                "4",
                            ),
                        ),
                        Some(
                            Tag(
                                "5",
                            ),
                        ),
                        None,
                        Some(
                            Text(
                                [
                                    (
                                        0,
                                        "f1",
                                    ),
                                ],
                            ),
                        ),
                        Some(
                            Text(
                                [
                                    (
                                        0,
                                        "f2",
                                    ),
                                    (
                                        3,
                                        "f3",
                                    ),
                                ],
                            ),
                        ),
                        None,
                        Some(
                            Boolean(
                                false,
                            ),
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
    ) -> (Box<str>, Arc<dyn Index>) {
        (
            name.into(),
            Arc::new((name, Arc::new(RwLock::new(events.into_iter().collect())))),
        )
    }
    type FakeIndex = (
        &'static str,
        Arc<RwLock<VecDeque<Option<IndexExportEvent>>>>,
    );
    impl IndexExport for FakeIndex {
        fn export(
            &self,
            _revision: BlobId,
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
            _revision: BlobId,
            _attribute: Option<crate::index::prelude::AttributeIndex>,
            _function: crate::query::expression::Func,
            _value: &crate::query::expression::TermValue,
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
            _revision: Option<BlobId>,
            _operations: &[crate::index::prelude::IndexStoreOperation],
        ) -> Box<dyn Send + Iterator<Item = crate::index::prelude::IndexStoreEvent>> {
            unimplemented!()
        }
    }

    #[test]
    fn structure() {
        let entry = Entry::new(
            "entry1",
            [
                ("created".into(), Arc::new(vec![EntryValue::new(93939393)])),
                (
                    "category".into(),
                    Arc::new(vec![EntryValue::new("story:fantasy")]),
                ),
                (
                    "mixung".into(),
                    Arc::new(vec![
                        EntryValue::text(["dwarf", "fairy"]),
                        EntryValue::new("mixtag"),
                        EntryValue::new(3),
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
                    Some(
                        Tag(
                            "story:fantasy",
                        ),
                    ),
                ],
                "created": [
                    Some(
                        Integer(
                            93939393,
                        ),
                    ),
                ],
                "mixung": [
                    Some(
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
                    ),
                    Some(
                        Tag(
                            "mixtag",
                        ),
                    ),
                    Some(
                        Integer(
                            3,
                        ),
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

        impl From<FutureProofValue> for Option<EntryValue> {
            fn from(value: FutureProofValue) -> Self {
                match value {
                    FutureProofValue::Known(value) => Some(value),
                    FutureProofValue::Future(value) => {
                        tracing::warn!("Ignoring unknown value on import {value:#?}");
                        None
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
                    Some(
                        Tag(
                            "mixtag",
                        ),
                    ),
                    Some(
                        Integer(
                            3,
                        ),
                    ),
                ],
                "futured": [
                    None,
                ],
            },
        }
        "#);

        // The `entry_for_import` can now be imported into the engine:
        let engine = Engine::builder().build();
        let mut write = engine.write().expect("writer");
        write.import(entry_for_import);
        for event in write.commit() {
            match event {
                crate::engine::WriteEvent::Load(load_event) => {
                    println!("loading {load_event:?}");
                    load_event.send_empty().expect("send");
                }
                crate::engine::WriteEvent::Save(save_event) => {
                    println!("saving {save_event:?}");
                }
                crate::engine::WriteEvent::Stats(_) => {}
            }
        }
    }
}
