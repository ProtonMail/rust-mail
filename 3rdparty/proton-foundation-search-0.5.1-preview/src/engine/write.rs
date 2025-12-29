use std::collections::{BTreeSet, HashMap};
use std::fmt::Debug;
use std::sync::atomic::Ordering;

use tracing::{info, instrument, warn};

use super::*;
use crate::document::Document;
use crate::index::collection::{CollectionStoreEvent, CollectionWriteOperation};
use crate::index::prelude::*;
use crate::processor::ProcessorError;
use crate::transaction::{LoadEvent, NoCache, ReleaseEvent, SaveEvent, TransactionState};

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

impl Write {
    pub(super) fn engine(&self) -> &InnerEngine {
        self.engine.0.as_ref()
    }
    pub(super) fn operations(&self) -> &[CollectionWriteOperation] {
        &self.operations
    }
}

/// Search engine write event
#[derive(Debug)]
pub enum WriteEvent {
    /// An entry attribute value has been inserted
    Modified(Box<str>),
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
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
    pub fn insert(&mut self, document: Document) -> Result<(), ProcessorError> {
        let (_size, entry) = self.engine.processor.process_document(document)?;

        self.import(entry);

        Ok(())
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
        HashMap<EntryIndex, Box<str>>,
    ),
    Indices(
        BTreeMap<Box<str>, Box<dyn Send + Iterator<Item = IndexStoreEvent>>>,
        HashMap<EntryIndex, Box<str>>,
    ),
}
impl Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init(arg0) => f.debug_tuple("Init").field(arg0).finish(),
            Self::Collection(_, arg1, arg2, arg3, arg4) => f
                .debug_tuple("Collection")
                .field(arg1)
                .field(arg2)
                .field(arg3)
                .field(arg4)
                .finish_non_exhaustive(),
            Self::Indices(_, identifiers) => f
                .debug_tuple("Indices")
                .field(identifiers)
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
    state: Option<TransactionState<NoCache<Manifest>, Manifest>>,
    /// indexed by the index ID, it indicates if an index has been modified
    modified: BTreeSet<Box<str>>,
    modified_collection: bool,
    stage: Stage,
}

impl Execution {
    fn new(writer: Write) -> Self {
        Self {
            state: Some(TransactionState::no_cache(
                MANIFEST.into(),
                Manifest::default,
            )),
            modified: Default::default(),
            modified_collection: false,
            stage: Stage::Init(writer.operations),
            guard: writer.engine,
        }
    }
}

impl Iterator for Execution {
    type Item = WriteEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            state,
            stage,
            modified,
            modified_collection,
            guard,
            ..
        } = self;

        let manifest = match state.as_mut()?.load()? {
            Ok(manifest) => manifest,
            Err(load) => return Some(WriteEvent::Load(load)),
        };

        loop {
            break match stage {
                Stage::Init(operations) => {
                    *stage = Stage::Collection(
                        Box::new(guard.collection.write(
                            manifest.collection_revision,
                            operations,
                            guard.current_batch.load(Ordering::Relaxed),
                        )),
                        std::mem::take(operations),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    );
                    continue;
                }
                Stage::Collection(events, operations, attrs, entries, identifiers) => {
                    if let Some(next) = events.next() {
                        match next {
                            CollectionStoreEvent::Entry { entry, identifier } => {
                                entries.insert(identifier.clone(), entry);
                                identifiers.insert(entry, identifier);
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
                                self.modified_collection = true;
                                manifest.active_blobs.insert(save.name.clone());
                                Some(WriteEvent::Save(save))
                            }
                            CollectionStoreEvent::Release(ReleaseEvent { name }) => {
                                manifest.active_blobs.remove(&name);
                                manifest.released_blobs.insert(name);
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
                                    index_ops
                                        .push(IndexStoreOperation::Remove(entries[&identifier]));
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
                                            manifest
                                                .index_revisions
                                                .get(id)
                                                .copied()
                                                .unwrap_or_default(),
                                            index_ops.as_slice(),
                                        ),
                                    )
                                })
                                .collect(),
                            std::mem::take(identifiers),
                        );
                        continue;
                    }
                }
                Stage::Indices(transactions, identifiers) => {
                    for (id, txn) in transactions.iter_mut() {
                        if let Some(next) = txn.next() {
                            return Some(match next {
                                IndexStoreEvent::Inserted { entry, .. }
                                | IndexStoreEvent::Removed { entry, .. } => {
                                    let Some(identifier) = identifiers.get(&entry) else {
                                        warn!("Missing ID for {entry:?} in {identifiers:?}");
                                        continue;
                                    };
                                    WriteEvent::Modified(identifier.clone())
                                }
                                IndexStoreEvent::Load(load) => WriteEvent::Load(load),
                                IndexStoreEvent::Save(save) => {
                                    info!(op = "save", id);
                                    modified.insert(id.clone());
                                    manifest.active_blobs.insert(save.name.clone());
                                    WriteEvent::Save(save)
                                }
                                IndexStoreEvent::Release(ReleaseEvent { name }) => {
                                    manifest.active_blobs.remove(&name);
                                    manifest.released_blobs.insert(name);
                                    continue;
                                }
                            });
                        }
                    }

                    if std::mem::take(modified_collection) {
                        manifest.collection_revision = manifest.collection_revision.wrapping_add(1);
                    }
                    if !modified.is_empty() {
                        for id in std::mem::take(modified) {
                            info!(
                                id,
                                modified = manifest
                                    .index_revisions
                                    .get(&id)
                                    .copied()
                                    .unwrap_or_default()
                            );
                            manifest
                                .index_revisions
                                .entry(id)
                                .and_modify(|rev| *rev = rev.wrapping_add(1))
                                .or_insert(1);
                        }

                        let (save, release) = state.take()?.save();
                        assert!(release.is_none(), "manifest is not revised");

                        Some(WriteEvent::Save(save))
                    } else {
                        None
                    }
                }
            };
        }
    }
}
