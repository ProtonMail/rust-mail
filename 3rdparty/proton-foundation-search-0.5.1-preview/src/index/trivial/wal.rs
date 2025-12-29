use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, channel};

use crate::entry::EntryValue;
use crate::index::prelude::wal::*;
use crate::index::prelude::*;
use crate::query::expression::Func;
use crate::query::option::QueryOptions;
use crate::transaction::{LoadEvent, SaveEvent};
use crate::wal_utils::generate_wal_timestamp;

/// WAL-based trivial index store that replaces transaction handling
#[derive(Debug, Clone)]
pub struct WALBasedTrivialIndexStore {
    /// In-memory WAL buffer for batching operations
    wal_buffer: Vec<(AttributeIndex, WALEntry)>,
    /// Manifest blob ID pointing to latest WAL blob
    manifest_blob_id: Option<String>,
    /// Index metadata for manifest
    metadata: HashMap<String, String>,
}

impl WALBasedTrivialIndexStore {
    /// Create a new WAL-based trivial index store
    pub fn new() -> Self {
        Self {
            wal_buffer: Vec::new(),
            manifest_blob_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Append a WAL entry to the buffer
    pub fn append_wal_entry(&mut self, attr: AttributeIndex, entry: WALEntry) {
        self.wal_buffer.push((attr, entry));
    }

    /// Get current WAL buffer size
    pub fn wal_buffer_size(&self) -> usize {
        self.wal_buffer.len()
    }

    /// Update manifest blob ID
    pub fn update_manifest_blob_id(&mut self, blob_id: String) {
        self.manifest_blob_id = Some(blob_id);
    }

    /// Get manifest metadata
    pub fn get_manifest_metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Add metadata entry
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
}

impl Default for WALBasedTrivialIndexStore {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexExport for WALBasedTrivialIndexStore {
    fn export(
        &self,
        _revision: u64,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        todo!()
    }
}
impl IndexStore for WALBasedTrivialIndexStore {
    fn id(&self) -> &str {
        // TODO: change to expand based on field or field combo
        "wal_trivial"
    }
    fn write(
        &self,
        _revision: u64,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>> {
        // Phase 1: Accumulate operations into WAL buffer
        let mut store = self.clone(); // Get mutable copy to modify buffer

        for op in operations {
            match op {
                IndexStoreOperation::Insert(entry, attr, value) => {
                    let indexed_value: &EntryValues = value.as_ref();

                    // Hehlper function to create WAL entries for a vector of values

                    for (value_idx, val) in indexed_value.iter().enumerate() {
                        let wal_entry = WALEntry::TrivialValue(TrivialValueEntry {
                            entry_index: *entry,
                            attribute_index: *attr,
                            value_index: ValueIndex(value_idx),
                            value: val.clone(),
                            timestamp: generate_wal_timestamp(),
                            operation_type: WALOperationType::Addition,
                        });
                        store.append_wal_entry(*attr, wal_entry);
                    }
                }
                IndexStoreOperation::Remove(entry) => {
                    // Convert to removal WAL entry
                    let wal_entry = WALEntry::TrivialValue(TrivialValueEntry {
                        entry_index: *entry,
                        attribute_index: AttributeIndex(0), // Default for removal
                        value_index: ValueIndex(0),
                        value: EntryValue::Empty,
                        timestamp: generate_wal_timestamp(),
                        operation_type: WALOperationType::Removal,
                    });
                    // Use AttributeIndex(0) as a wildcard to indicate removal from all trivial indices (assuming we allow split)
                    // TODO: Consider changing IndexStoreOperation::Remove to include AttributeIndex or wildcard
                    store.append_wal_entry(AttributeIndex(0), wal_entry);
                }
            }
        }

        // Phase 2: Generate save events for each attribute's WAL entries
        let timestamp = generate_wal_timestamp();
        let mut save_events = Vec::new();

        // Group WAL entries by attribute
        let mut entries_by_attribute = HashMap::new();
        for (attr, entry) in store.wal_buffer {
            entries_by_attribute
                .entry(attr)
                .or_insert_with(Vec::new)
                .push(entry);
        }

        // Create a save event for each attribute's entries
        for (attr, entries) in entries_by_attribute {
            let wal_file_name = format!("trivial_val_Attribute[{}]_{}", attr.0, timestamp);
            let entries_clone = entries.clone();
            let save_event = SaveEvent {
                name: wal_file_name.into(),
                recv: Box::new(move |_| WALFormat::to_json(&entries_clone)),
            };
            save_events.push(save_event);
        }

        Box::new(save_events.into_iter().map(IndexStoreEvent::Save))
    }

    fn reset(&self) {
        todo!("interior mutability")
        // self.wal_buffer.clear();
        // self.metadata.clear();
    }
}

/// WAL-based finder that follows the same pattern as the text WALFinder
#[derive(Default)]
enum TrivialWALFinder {
    /// Loading WAL data from storage
    LoadingWAL {
        load_events: VecDeque<IndexSearchEvent>,
        #[allow(dead_code)]
        store_ref: std::sync::Arc<std::sync::Mutex<WALBasedTrivialIndexStore>>,
    },
    /// Iterating through search results
    #[allow(dead_code)]
    Iterating { results: VecDeque<IndexSearchEvent> },
    #[default]
    Done,
}

impl TrivialWALFinder {
    fn new(
        _revision: u64,
        store: &WALBasedTrivialIndexStore,
        attribute: Option<AttributeIndex>,
    ) -> Self {
        let (load_events, _rx) = Self::create_wal_load_events(attribute);
        let store_ref = std::sync::Arc::new(std::sync::Mutex::new(store.clone()));

        TrivialWALFinder::LoadingWAL {
            load_events,
            store_ref,
        }
    }

    fn create_wal_load_events(
        attribute: Option<AttributeIndex>,
    ) -> (VecDeque<IndexSearchEvent>, Receiver<()>) {
        let mut load_events = VecDeque::new();
        let (_tx, rx) = channel();

        // Determine which attributes to load
        tracing::info!(
            "🔍 TrivialWALFinder: Filter attribute resolution - attribute = {:?}",
            attribute
        );

        let attributes_to_load = if let Some(attr) = attribute {
            // If filter specifies an attribute, load that one
            vec![attr]
        } else {
            vec![]
        };

        // Create load events for each attribute
        tracing::debug!(
            "🔍 Creating trivial load events for {} attributes: {:?}",
            attributes_to_load.len(),
            attributes_to_load
        );

        for attr in attributes_to_load {
            let wal_file_name = format!("trivial_val_Attribute[{}]", attr.0).into_boxed_str();
            tracing::trace!(
                "TrivialWALFinder creating LoadEvent for {} (attr={})",
                wal_file_name,
                attr.0
            );

            let load_event = LoadEvent {
                name: wal_file_name,
                send: Box::new(move |_serdes, _data| {
                    // This callback will be called by the host with the WAL data
                    // For now, just return Ok - the actual WAL processing will happen later
                    Ok(())
                }),
            };
            load_events.push_back(IndexSearchEvent::Load(load_event));
        }

        (load_events, rx)
    }
}

impl Iterator for TrivialWALFinder {
    type Item = IndexSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TrivialWALFinder::LoadingWAL { load_events, .. } => {
                if let Some(event) = load_events.pop_front() {
                    Some(event)
                } else {
                    *self = TrivialWALFinder::Done;
                    None
                }
            }
            TrivialWALFinder::Iterating { results } => {
                if let Some(result) = results.pop_front() {
                    Some(result)
                } else {
                    *self = TrivialWALFinder::Done;
                    None
                }
            }
            TrivialWALFinder::Done => None,
        }
    }
}

impl IndexSearch for WALBasedTrivialIndexStore {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        _function: Func,
        value: &Value,
        _options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        // Smart routing: only handle values that trivial indices can process
        if !self.can_handle_value(value) {
            tracing::debug!(
                "WALBasedTrivialIndexStore: Skipping incompatible value type: {:?}",
                value
            );
            return None;
        }

        // Use the TrivialWALFinder pattern similar to the text WALFinder
        Some(Box::new(TrivialWALFinder::new(revision, self, attribute)))
    }
}

impl WALBasedTrivialIndexStore {
    /// Check if this trivial index can handle the given value type
    fn can_handle_value(&self, value: &Value) -> bool {
        match value {
            Value::Integer(_) => true, // Trivial index handles integers (like timestamps)
            Value::Boolean(_) => true, // Trivial index handles booleans
            Value::Tag(_) => true,     // Trivial index handles tags/strings as exact matches
            Value::Text(_) => false,   // Text values should go to text index
        }
    }
}
