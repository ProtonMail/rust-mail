//! WAL write operations - same pattern as Writer but using WAL indices
//!
//! WriterWAL follows the exact same pattern as Writer but delegates to WAL indices
//! instead of regular indices. WAL indices handle their own WAL storage.

use std::collections::HashMap;

use crate::document::Document;
use crate::engine::{Write, WriteEvent};
use crate::index::collection::{CollectionWriteOperation, Writer};
use crate::index::prelude::*;
use crate::processor::ProcessorError;
use crate::transaction::SaveEvent;
use crate::wal_utils::generate_wal_timestamp;

/// WAL manifest entry mapping attributes to their WAL files
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct WALManifest {
    /// Collection revision
    collection_revision: u64,
    /// Collection file name (timestamped)
    collection_file: Option<String>,
    /// Map of attribute index to list of WAL file names
    attribute_wal_files: HashMap<AttributeIndex, Vec<String>>,
    /// Timestamp when manifest was created
    timestamp: u64,
}

impl WALManifest {
    fn new(collection_revision: u64) -> Self {
        Self {
            collection_revision,
            collection_file: None,
            attribute_wal_files: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_millis() as u64,
        }
    }

    fn set_collection_file(&mut self, collection_file: String) {
        self.collection_file = Some(collection_file);
    }

    fn add_wal_file(&mut self, attribute: AttributeIndex, wal_file_name: String) {
        self.attribute_wal_files
            .entry(attribute)
            .or_default()
            .push(wal_file_name);
    }
}

/// WAL write operation - same pattern as Writer but with WAL indices
#[derive(Debug)]
pub struct WriterWAL {
    inner: Write,
}

impl WriterWAL {
    /// Create a new WAL write operation
    pub(super) fn new(base: Write) -> Self {
        Self { inner: base }
    }

    /// Insert a document - same as Writer
    pub fn insert(&mut self, document: Document) -> Result<(), ProcessorError> {
        self.inner.insert(document)
    }

    /// Remove a document - same as Writer
    pub fn remove(&mut self, identifier: Box<str>) {
        self.inner.remove(&identifier);
    }

    /// Commit WAL operation - same pattern as Writer but uses WAL indices
    pub fn commit(self) -> impl Iterator<Item = WriteEvent> {
        WALExecution::new(self)
    }
}

/// WAL execution iterator - same pattern as Execution but with WAL indices
struct WALExecution {
    writer: WriterWAL,
    stage: WALStage,
    // WAL indices (instead of regular indices)
    wal_indices: Vec<Box<dyn Send + Iterator<Item = IndexStoreEvent>>>,
    identifiers: HashMap<EntryIndex, Box<str>>,
    // Store attributes for index operations
    attributes: HashMap<Box<str>, AttributeIndex>,
    /// Track WAL file names for manifest creation
    wal_files: Vec<String>,
    /// Track collection file name for manifest
    collection_file_name: Option<String>,
}

enum WALStage {
    /// Initialize collection processing (same as original Writer)
    Init,
    /// Process collection operations (same as original Writer)
    Collection(Writer),
    /// Process WAL indices (NO load phase needed)
    Indices,
    /// Create WAL manifest
    Manifest,
    /// Done
    Done,
}

impl std::fmt::Debug for WALStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init => write!(f, "Init"),
            Self::Collection(_) => write!(f, "Collection(<iterator>)"),
            Self::Indices => write!(f, "Indices"),
            Self::Manifest => write!(f, "Manifest"),
            Self::Done => write!(f, "Done"),
        }
    }
}

impl WALExecution {
    fn new(writer: WriterWAL) -> Self {
        Self {
            writer,
            stage: WALStage::Init,
            wal_indices: vec![],
            identifiers: HashMap::new(),
            attributes: HashMap::new(),
            wal_files: Vec::new(),
            collection_file_name: None,
        }
    }

    /// Parse attribute index from WAL filename
    fn parse_attribute_from_wal_filename(&self, filename: &str) -> Option<AttributeIndex> {
        // Parse filenames of the form "text_val_Attribute[1]_1756316905961282"
        let start = filename.find("Attribute[")?;
        let end = filename[start..].find(']')?;
        let attr_str = &filename[start + 10..start + end];
        let attr_num = attr_str.parse::<u8>().ok()?;
        Some(AttributeIndex(attr_num))
    }
}

impl Iterator for WALExecution {
    type Item = WriteEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.stage {
                WALStage::Init => {
                    tracing::trace!("WAL Writer: Starting Init stage - same as original Writer");

                    // Don't create EntryIndex mappings here - let the collection assign them
                    // The collection will assign the correct EntryIndex values based on the actual document IDs

                    // Still need to create attribute mappings for index operations
                    for collection_op in self.writer.inner.operations() {
                        match collection_op {
                            CollectionWriteOperation::Insert(_, values) => {
                                for field in values.keys() {
                                    let attr_index = self.attributes.len() as u8;
                                    let attr = AttributeIndex(attr_index);
                                    let actual_attr =
                                        self.attributes.entry(field.clone()).or_insert(attr);
                                    tracing::trace!(
                                        "WAL Writer: Pre-mapped Attribute: '{}' -> Attribute({})",
                                        field,
                                        actual_attr.0
                                    );
                                }
                            }
                            CollectionWriteOperation::Remove(_) => {
                                // Removals don't create new mappings
                            }
                        }
                    }

                    // Call collection.write() for collection file persistence (same as original Writer)
                    let collection_events = self.writer.inner.engine().collection.write(
                        0,
                        self.writer.inner.operations(),
                        self.writer
                            .inner
                            .engine()
                            .current_batch
                            .load(std::sync::atomic::Ordering::Relaxed),
                    );

                    self.stage = WALStage::Collection(collection_events);
                    continue;
                }
                WALStage::Collection(events) => {
                    // Same pattern as original Writer: process CollectionStoreEvent
                    if let Some(next) = events.next() {
                        return Some(match next {
                            crate::index::collection::CollectionStoreEvent::Entry {
                                entry,
                                identifier,
                            } => {
                                tracing::trace!(
                                    "WAL Writer: Collection Entry: '{}' -> Entry({})",
                                    identifier,
                                    entry.0
                                );
                                self.identifiers.insert(entry, identifier.clone());
                                WriteEvent::Modified(identifier)
                            }
                            crate::index::collection::CollectionStoreEvent::Attribute {
                                attribute,
                                name,
                            } => {
                                tracing::trace!(
                                    "WAL Writer: Collection Attribute: '{}' -> Attribute({})",
                                    name,
                                    attribute.0
                                );
                                // Store attribute mapping for index operations
                                self.attributes.insert(name.clone(), attribute);
                                // Attributes don't directly generate WriteEvent::Modified in original
                                // Continue to next event
                                return self.next();
                            }
                            crate::index::collection::CollectionStoreEvent::Load(load) => {
                                tracing::trace!("WAL Writer: Collection Load: {}", load.name);
                                WriteEvent::Load(load)
                            }
                            crate::index::collection::CollectionStoreEvent::Save(save) => {
                                tracing::trace!("WAL Writer: Collection Save: {}", save.name);

                                // Convert collection save to use timestamp instead of revision
                                if save.name.starts_with("collection r") {
                                    let timestamp = generate_wal_timestamp();
                                    let timestamped_collection_name =
                                        format!("collection_{}", timestamp);

                                    tracing::trace!(
                                        "WAL Writer: Converting collection name from {} to {}",
                                        save.name,
                                        timestamped_collection_name
                                    );

                                    // Track the collection file for manifest
                                    self.collection_file_name =
                                        Some(timestamped_collection_name.clone());

                                    // Create new SaveEvent with timestamped name
                                    let timestamped_save = SaveEvent {
                                        name: timestamped_collection_name.into_boxed_str(),
                                        recv: save.recv,
                                    };

                                    WriteEvent::Save(timestamped_save)
                                } else {
                                    WriteEvent::Save(save)
                                }
                            }
                            crate::index::collection::CollectionStoreEvent::Release(release) => {
                                tracing::trace!("WAL Writer: Collection Release: {}", release.name);
                                // Continue to next event - releases don't generate WriteEvents
                                return self.next();
                            }
                        });
                    }

                    // Collection processing complete, move to indices
                    tracing::trace!(
                        "WAL Writer: Collection processing complete, moving to Indices stage"
                    );

                    // Now convert the original collection operations to index operations
                    // (This is what was working before)
                    let mut index_ops = vec![];
                    tracing::trace!(
                        "WAL Writer: Converting collection operations to index operations"
                    );

                    for collection_op in self.writer.inner.operations() {
                        match collection_op {
                            CollectionWriteOperation::Insert(identifier, values) => {
                                if let Some(&entry) = self
                                    .identifiers
                                    .iter()
                                    .find(|(_, id)| *id == identifier)
                                    .map(|(e, _)| e)
                                {
                                    tracing::trace!(
                                        "WAL Writer: Processing Insert for '{}' -> Entry({})",
                                        identifier,
                                        entry.0
                                    );

                                    for (field, value) in values {
                                        if let Some(&attr) = self.attributes.get(field) {
                                            tracing::trace!(
                                                "WAL Writer: Creating IndexStoreOperation::Insert Entry({}) Attr({}) for field '{}'",
                                                entry.0,
                                                attr.0,
                                                field
                                            );
                                            index_ops.push(IndexStoreOperation::Insert(
                                                entry,
                                                attr,
                                                value.clone(),
                                            ));
                                        }
                                    }
                                }
                            }
                            CollectionWriteOperation::Remove(identifier) => {
                                if let Some(&entry) = self
                                    .identifiers
                                    .iter()
                                    .find(|(_, id)| *id == identifier)
                                    .map(|(e, _)| e)
                                {
                                    tracing::trace!(
                                        "WAL Writer: Creating IndexStoreOperation::Remove for '{}' -> Entry({})",
                                        identifier,
                                        entry.0
                                    );
                                    index_ops.push(IndexStoreOperation::Remove(entry));
                                }
                            }
                        }
                    }

                    tracing::trace!(
                        "WAL Writer: Generated {} index operations from collection data",
                        index_ops.len()
                    );
                    self.stage = WALStage::Indices;

                    // Use the base engine's indices instead of hardcoding them
                    tracing::trace!("WAL Writer: Using base engine's indices for WAL operations");
                    self.wal_indices = self
                        .writer
                        .inner
                        .engine()
                        .indices
                        .values()
                        .map(|index| index.write(0, &index_ops))
                        .collect();

                    continue;
                }

                WALStage::Indices => {
                    tracing::trace!("WAL Writer: Processing Indices stage");
                    // Same pattern as regular Writer - iterate through index events
                    for (store_idx, txn) in self.wal_indices.iter_mut().enumerate() {
                        if let Some(next) = txn.next() {
                            tracing::trace!(
                                "WAL Writer: Index store {} emitted event: {:?}",
                                store_idx,
                                match &next {
                                    IndexStoreEvent::Inserted { entry, .. } =>
                                        format!("Inserted(entry={})", entry.0),
                                    IndexStoreEvent::Removed { entry, .. } =>
                                        format!("Removed(entry={})", entry.0),
                                    IndexStoreEvent::Load(load) => format!("Load({})", load.name),
                                    IndexStoreEvent::Save(save) => format!("Save({})", save.name),
                                    IndexStoreEvent::Release(release) =>
                                        format!("Release({})", release.name),
                                }
                            );

                            return Some(match next {
                                IndexStoreEvent::Inserted { entry, .. }
                                | IndexStoreEvent::Removed { entry, .. } => {
                                    let identifier = self.identifiers.get(&entry)?;
                                    tracing::trace!(
                                        "WAL Writer: Document modified: '{}'",
                                        identifier
                                    );
                                    WriteEvent::Modified(identifier.clone())
                                }
                                IndexStoreEvent::Load(load) => {
                                    // WAL indices should never emit Load events
                                    tracing::warn!(
                                        "WAL Writer: Unexpected Load event from WAL store: {}",
                                        load.name
                                    );
                                    continue;
                                }
                                IndexStoreEvent::Save(save) => {
                                    // WAL indices emit Save events for WAL blobs
                                    tracing::trace!("WAL Writer: WAL blob save: {}", save.name);
                                    // Track WAL file name for manifest creation
                                    self.wal_files.push(save.name.to_string());
                                    WriteEvent::Save(save)
                                }
                                IndexStoreEvent::Release(release) => {
                                    // Handle cleanup if needed
                                    tracing::trace!("WAL Writer: Release event: {}", release.name);
                                    continue;
                                }
                            });
                        }
                    }

                    // All indices processed
                    tracing::trace!("WAL Writer: All indices processed, moving to Manifest stage");

                    // Move to manifest creation stage
                    self.stage = WALStage::Manifest;
                    continue;
                }
                WALStage::Manifest => {
                    tracing::trace!("WAL Writer: Creating WAL manifest");

                    // Create WAL manifest with all collected WAL files
                    let mut manifest = WALManifest::new(1); // collection revision 1

                    // Set collection file if we have one
                    if let Some(ref collection_file) = self.collection_file_name {
                        manifest.set_collection_file(collection_file.clone());
                    }

                    // Parse WAL file names to extract attribute information
                    for wal_file_name in &self.wal_files {
                        if let Some(attr) = self.parse_attribute_from_wal_filename(wal_file_name) {
                            manifest.add_wal_file(attr, wal_file_name.clone());
                        }
                    }

                    // Create manifest save event - append to existing manifest if it exists
                    let collection_info = manifest
                        .collection_file
                        .as_ref()
                        .map(|file| format!("Collection File: {}\n", file))
                        .unwrap_or_default();

                    let manifest_text = format!(
                        "WAL Manifest\n\
                         Collection Revision: {}\n\
                         {}Timestamp: {}\n\
                         WAL Files:\n{}",
                        manifest.collection_revision,
                        collection_info,
                        manifest.timestamp,
                        manifest
                            .attribute_wal_files
                            .iter()
                            .map(|(attr, files)| {
                                files
                                    .iter()
                                    .map(|file| format!("  Attribute[{}]: {}", attr.0, file))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    );

                    // Use a timestamped name for the manifest for correlation
                    let timestamp = generate_wal_timestamp();
                    let timestamped_manifest_name = format!("wal_manifest_{}", timestamp);

                    let manifest_save = SaveEvent {
                        name: timestamped_manifest_name.into(),
                        recv: Box::new(move |_| Ok(manifest_text.as_bytes().to_vec())),
                    };

                    // Move to done stage
                    self.stage = WALStage::Done;

                    // Return the manifest save event
                    return Some(WriteEvent::Save(manifest_save));
                }
                WALStage::Done => return None,
            }
        }
    }
}
