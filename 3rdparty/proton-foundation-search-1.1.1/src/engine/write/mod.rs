use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::ControlFlow;
use std::sync::atomic::Ordering;

use tracing::{debug, instrument, warn};

use super::*;
use crate::blob::{Blob, LoadEvent, SaveEvent};
use crate::document::Document;
use crate::entry::Entry;
use crate::index::collection::{CollectionStoreEvent, CollectionWriteOperation};
use crate::index::prelude::*;
mod process_document;

#[cfg(test)]
mod process_document_tests;

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Creates and returns a writer.
    /// Returns None if another write is in progress.
    /// It is the responsibility of the app to serialize writes
    /// (as this can differ in async/multi-thread/single-thread use cases).
    pub fn write(&self) -> Option<Write> {
        if self.inner.writer.swap(true, Ordering::AcqRel) {
            warn!("write() already writing");
            return None;
        }
        Some(Write::new(self))
    }
}

/// Search engine Write operation
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Write {
    engine: EngineWriteGuard,
    pub(super) operations: Vec<CollectionWriteOperation>,
}

/// Search engine write event
#[derive(Debug)]
pub enum WriteEvent {
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
    /// Updated stats of the engine
    Stats(Stats),
}

impl Write {
    #[instrument(skip_all)]
    fn new(engine: &Engine) -> Self {
        Self {
            engine: EngineWriteGuard(engine.inner.clone()),
            operations: vec![],
        }
    }

    /// Prepares the worker for inserting a new document when committing.
    #[inline]
    #[tracing::instrument(skip_all)]
    pub fn insert(&mut self, document: Document) {
        let entry = document.process(self.engine.processor.as_ref());
        self.import(entry);
    }

    /// Prepares the worker for inserting a new import when committing.
    #[inline]
    #[tracing::instrument(skip_all)]
    pub fn import(&mut self, entry: Entry) {
        let (identifier, attributes) = entry.into();
        self.operations
            .push(CollectionWriteOperation::Insert(identifier, attributes));
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Write {
    /// Prepares the worker for removing a document when committing.
    #[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
    pub fn remove(&mut self, identifier: &str) {
        // tracing #[instrument] messes with wasm bindings here:
        // __wbindgen_describe_write_remove: Condition failed: `address % 4 == 0` (2 vs 0)
        // the span avoids this issue
        let _span = tracing::span!(tracing::Level::TRACE, "remove", identifier);
        self.operations
            .push(CollectionWriteOperation::Remove(identifier.into()));
    }

    /// Commit the transaction by fully exhausting the returned iterator, serving Load and Save events with storage
    pub fn commit(self) -> Execution {
        Execution::new(self)
    }
}

enum Stage {
    Init(Vec<CollectionWriteOperation>),
    Collection(
        Box<dyn Send + Iterator<Item = CollectionStoreEvent>>,
        Vec<CollectionWriteOperation>,
        HashMap<Box<str>, AttributeIndex>,
        HashMap<Box<str>, EntryIndex>,
        Stats,
    ),
    Indices(
        BTreeMap<Box<str>, Box<dyn Send + Iterator<Item = IndexStoreEvent>>>,
        Option<Stats>,
    ),
}
impl Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init(arg0) => f.debug_tuple("Init").field(arg0).finish(),
            Self::Collection(_, arg1, arg2, arg3, stats) => f
                .debug_tuple("Collection")
                .field(arg1)
                .field(arg2)
                .field(arg3)
                .field(stats)
                .finish_non_exhaustive(),
            Self::Indices(_, stats) => f
                .debug_tuple("Indices")
                .field(stats)
                .finish_non_exhaustive(),
        }
    }
}

/// A write transaction iterator
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Execution {
    #[allow(unused, reason = "drops the writer flag")]
    guard: EngineWriteGuard,
    blob: Blob<Manifest>,
    manifest: Option<Manifest>,
    modified: bool,
    stage: Stage,
}

impl Execution {
    fn new(writer: Write) -> Self {
        Self {
            blob: Blob::new(MANIFEST_BLOB_ID, MANIFEST),
            manifest: None,
            modified: Default::default(),
            stage: Stage::Init(writer.operations),
            guard: writer.engine,
        }
    }
}

impl Iterator for Execution {
    type Item = WriteEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            blob,
            stage,
            modified,
            guard,
            ..
        } = self;

        let manifest = match &mut self.manifest {
            Some(manifest) => manifest,
            None => match blob.fetch_and_free() {
                ControlFlow::Continue(manifest) => self.manifest.insert(manifest.as_ref().clone()),
                ControlFlow::Break(load) => return Some(WriteEvent::Load(load)),
            },
        };

        loop {
            break match stage {
                Stage::Init(operations) => {
                    *stage = Stage::Collection(
                        Box::new(
                            guard
                                .collection
                                .write(manifest.collection_revision, operations),
                        ),
                        std::mem::take(operations),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    );
                    continue;
                }
                Stage::Collection(events, operations, attrs, entries, stats) => {
                    if let Some(next) = events.next() {
                        match next {
                            CollectionStoreEvent::Stats(s) => {
                                *stats = s;
                                continue;
                            }
                            CollectionStoreEvent::Entry { entry, identifier } => {
                                entries.insert(identifier.clone(), entry);
                                continue;
                            }
                            CollectionStoreEvent::Attribute {
                                attribute,
                                name: field,
                            } => {
                                attrs.insert(field, attribute);
                                continue;
                            }
                            CollectionStoreEvent::Load(load) => Some(WriteEvent::Load(load)),
                            CollectionStoreEvent::Save(save) => {
                                manifest.collection_revision = Some(save.id());
                                manifest.active_blobs.insert(save.id());
                                Some(WriteEvent::Save(save))
                            }
                            CollectionStoreEvent::Release(release) => {
                                let id = release.id();
                                manifest.active_blobs.remove(&id);
                                manifest.released_blobs.insert(id);
                                continue;
                            }
                        }
                    } else {
                        // All collection operations have been done, let's do indices

                        let mut index_ops = vec![];

                        for collection_op in std::mem::take(operations) {
                            match collection_op {
                                CollectionWriteOperation::Insert(identifier, values) => {
                                    let entry = entries[&identifier];
                                    index_ops.extend(values.into_iter().map(|(field, value)| {
                                        IndexStoreOperation::Insert(entry, attrs[&field], value)
                                    }));
                                }
                                CollectionWriteOperation::Remove(identifier) => {
                                    if let Some(entry_id) = entries.get(&identifier) {
                                        index_ops.push(IndexStoreOperation::Remove(*entry_id));
                                    }
                                }
                            }
                        }

                        *stage = Stage::Indices(
                            guard
                                .indices
                                .iter()
                                .map(|(id, index)| {
                                    (
                                        id.clone(),
                                        index.write(
                                            manifest.index_revisions.get(id).copied(),
                                            index_ops.as_slice(),
                                        ),
                                    )
                                })
                                .collect(),
                            Some(std::mem::take(stats)),
                        );
                        continue;
                    }
                }
                Stage::Indices(transactions, stats) => {
                    for (id, txn) in transactions {
                        for next in txn.by_ref() {
                            return Some(match next {
                                IndexStoreEvent::Inserted { .. }
                                | IndexStoreEvent::Removed { .. } => {
                                    continue;
                                }
                                IndexStoreEvent::Load(load) => WriteEvent::Load(load),
                                IndexStoreEvent::Save(save) => {
                                    debug!(op = "save", id);
                                    manifest.active_blobs.insert(save.id());
                                    WriteEvent::Save(save)
                                }
                                IndexStoreEvent::Release(release) => {
                                    let id = release.id();
                                    manifest.active_blobs.remove(&id);
                                    manifest.released_blobs.insert(id);
                                    continue;
                                }
                                IndexStoreEvent::Revision(revision) => {
                                    *modified = true;
                                    manifest.index_revisions.insert(id.clone(), revision.id());
                                    continue;
                                }
                            });
                        }
                    }

                    if std::mem::take(modified) {
                        let manifest = std::mem::take(manifest);
                        let (release, save) = blob.save(MANIFEST_BLOB_ID, Arc::new(manifest));
                        assert!(release.is_none(), "manifest is not revised");
                        Some(WriteEvent::Save(save))
                    } else {
                        stats.take().map(WriteEvent::Stats)
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Engine;

    #[test]
    fn write_implements_send() {
        fn sendy(_send: impl Send) {}
        sendy(Engine::builder().build().write());
        sendy(Engine::builder().build().write().expect("write").commit());
    }
}
