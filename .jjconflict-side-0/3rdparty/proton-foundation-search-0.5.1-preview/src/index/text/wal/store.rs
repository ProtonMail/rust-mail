use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use tracing::trace;

use super::super::inner::AdditiveTextIndex;
use crate::index::prelude::wal::*;
use crate::index::prelude::*;
use crate::index::text::inner::TextIndex;
use crate::index::text::inner::filter::TextFilter;
use crate::index::text::search::filter::{TextFilterSansIo, TextSearch};
use crate::index::text::wal::ReconstructedTextData;
use crate::query::expression::Func;
use crate::query::option::QueryOptions;
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};
use crate::transaction::{LoadEvent, SaveEvent};
use crate::wal_utils::{generate_text_addition_wal_entries, generate_wal_timestamp};

/// WAL-based text index store with persistent caching
#[derive(Debug, Clone)]
pub struct WALBasedTextIndexStore {
    /// In-memory WAL buffer for batching operations
    wal_buffer: Vec<(AttributeIndex, WALEntry)>,
    /// Current reconstructed index state
    current_state: ReconstructedTextData,
    /// Shared persistent cache for the built index
    cached_index: Arc<ArcSwapOption<TextIndex>>,
    /// Manifest blob ID pointing to latest WAL blob
    manifest_blob_id: Option<String>,
    /// Index metadata for manifest
    metadata: HashMap<String, String>,
}

fn create_chained_load_event(
    request_name: String,
    shared_index: Arc<Mutex<AdditiveTextIndex>>,
    result_tx: Sender<TextIndex>,
    chain_tx: Sender<IndexSearchEvent>,
) -> LoadEvent {
    let request_name_clone = request_name.clone();
    LoadEvent {
        name: request_name.into_boxed_str(),
        send: Box::new(move |_serdes, data| {
            let (data_without_metadata, metadata) = WALFormat::extract_metadata(&data);

            let wal_entries: Vec<WALEntry> = WALFormat::from_json(&data_without_metadata)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            // Merge into shared index
            let new_additive_index = AdditiveTextIndex::from_wal_entries(&wal_entries);
            if let Ok(mut index) = shared_index.lock() {
                index.merge(&new_additive_index);
            }

            // Check for continuation
            match metadata.as_ref().and_then(|m| m.next_timestamp.as_ref()) {
                Some(timestamp) if timestamp != "EOF" => {
                    // Continue chain - extract base name (everything before the last underscore)
                    let base_name = if let Some(pos) = request_name_clone.rfind('_') {
                        &request_name_clone[..pos] // Everything before the last underscore
                    } else {
                        &request_name_clone // No underscore, use full name
                    };
                    let next_request = format!("{base_name}_{timestamp}");
                    let next_event = create_chained_load_event(
                        next_request,
                        shared_index,
                        result_tx,
                        chain_tx.clone(),
                    );
                    chain_tx.send(IndexSearchEvent::Load(next_event)).ok();
                }
                _ => {
                    // Send final result
                    if let Ok(index) = shared_index.lock() {
                        let final_index = index.to_hierarchical();
                        result_tx.send(final_index).ok();
                    }
                }
            }

            Ok(())
        }),
    }
}

impl WALBasedTextIndexStore {
    /// Create a new WAL-based text index store
    pub fn new() -> Self {
        Self {
            wal_buffer: Vec::new(),
            current_state: ReconstructedTextData::default(),
            cached_index: Arc::new(ArcSwapOption::empty()),
            manifest_blob_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Store a built index for persistent caching
    pub fn store_built_index(&self, index: TextIndex) {
        self.cached_index.store(Some(Arc::new(index)));
        tracing::info!("✅ Index cached persistently");
    }

    /// Get the cached index if available
    pub fn get_cached_index(&self) -> Option<Arc<TextIndex>> {
        self.cached_index.load_full()
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

impl Default for WALBasedTextIndexStore {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexStore for WALBasedTextIndexStore {
    fn id(&self) -> &str {
        // TODO: change to expand based on field or field combo
        "wal_text"
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
                    // Generate all three types of WAL entries for text insertion
                    let wal_entries =
                        generate_text_addition_wal_entries(*entry, *attr, value.as_ref());
                    for wal_entry in wal_entries {
                        store.append_wal_entry(*attr, wal_entry);
                    }
                }
                IndexStoreOperation::Remove(entry) => {
                    // Convert to removal WAL entry (simplified for now)
                    let wal_entry = WALEntry::TokenRef(TokenRefEntry {
                        token: format!("removed_{}", entry.0),
                        token_ref: entry.0 as u64,
                        timestamp: generate_wal_timestamp(),
                        operation_type: WALOperationType::Removal,
                    });
                    // Use AttributeIndex(0) as a wildcard to indicate removal from all indices
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
            let wal_file_name = format!("text_val_Attribute[{}]_{}", attr.0, timestamp);
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
        self.cached_index.store(None);
        todo!("interior mutability")
        // self.wal_buffer.clear();
        // self.current_state = ReconstructedTextData::default();
        // self.manifest_blob_id = None;
        // self.metadata.clear();
    }
}

impl WALBasedTextIndexStore {
    /// Check if this text index can handle the given value type
    /// no more spray and pray
    fn can_handle_value(&self, value: &Value) -> bool {
        match value {
            Value::Text(_) => true,     // Text index handles text values
            Value::Tag(_) => true,      // Text index can also handle tags as text
            Value::Integer(_) => false, // Integers should go to trivial index
            Value::Boolean(_) => false, // Booleans should go to trivial index
        }
    }
}

impl IndexExport for WALBasedTextIndexStore {
    fn export(
        &self,
        _revision: u64,
    ) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>> {
        todo!()
    }
}
impl IndexSearch for WALBasedTextIndexStore {
    fn search(
        &self,
        revision: u64,
        attribute: Option<AttributeIndex>,
        function: Func,
        value: &Value,
        options: &QueryOptions,
    ) -> Option<Box<dyn 'static + Send + Iterator<Item = IndexSearchEvent>>> {
        // Smart routing: only handle text values
        if !self.can_handle_value(value) {
            tracing::debug!(
                "WALBasedTextIndexStore: Skipping non-text value type: {:?}",
                value
            );
            return None;
        }

        // Create a filter based on the function and value (same as original TextIndexSansIo)
        let filter = match function {
            Func::Matches => TextSearch {
                filter: TextFilter::matches(
                    value.to_string(),
                    MaximumDistance::get(options),
                    MinimumSimilarity::get(options),
                ),
                attribute,
            },
            Func::Equals => TextSearch {
                filter: TextFilter::equals(value.to_string()),
                attribute,
            },
            Func::Prefix => TextSearch {
                filter: TextFilter::starts_with(value.to_string()),
                attribute,
            },
            Func::LessThan
            | Func::LessThanOrEqual
            | Func::GreaterThan
            | Func::GreaterThanOrEqual => return None,
        };

        // Use the WALFinder pattern similar to the original Finder
        Some(Box::new(WALFinder::new(revision, self, filter)))
    }
}

/// WAL-based finder that follows the same pattern as the original Finder
#[derive(Default)]
enum WALFinder<F> {
    /// Loading WAL data from storage
    LoadingWAL {
        filter: F,
        loading_receiver: Option<Receiver<TextIndex>>,
        load_events: VecDeque<IndexSearchEvent>,
        chained_receiver: Option<Receiver<IndexSearchEvent>>,
        store_ref: std::sync::Arc<std::sync::Mutex<WALBasedTextIndexStore>>,
    },
    /// Iterating through search results
    Iterating { results: VecDeque<IndexSearchEvent> },
    #[default]
    Done,
}

impl<F> WALFinder<F>
where
    F: TextFilterSansIo,
{
    fn new(revision: u64, store: &WALBasedTextIndexStore, filter: F) -> Self {
        trace!(
            revision = revision,
            "creating WALFinder for WALBasedTextIndexStore"
        );

        // Check if we have a cached index available
        if let Some(cached_index) = store.get_cached_index() {
            trace!("using persistent cached index for search");
            tracing::info!("✅ Using persistent cached index for search");
            return Self::Iterating {
                results: filter.get(&cached_index).collect(),
            };
        }

        // No cached index available, need to load WAL data
        {
            // Need to load WAL data
            tracing::info!("no built index, loading WAL data");
            let (load_events, loading_receiver, chained_receiver) =
                Self::create_wal_load_events(&filter);

            // Create a shared reference to the store for caching
            let store_ref = std::sync::Arc::new(std::sync::Mutex::new(store.clone()));

            // With an async loading option, we could start background processing here
            // and return partial results immediately while chained loading continues
            // keeping the engine sync we could offer an out of process index builder that the app could run and then replace the built index via an API
            tracing::info!(
                "Starting chained loading - queries could potentally be made to execute immediately"
            );

            Self::LoadingWAL {
                filter,
                loading_receiver: Some(loading_receiver),
                load_events,
                chained_receiver: Some(chained_receiver),
                store_ref,
            }
        }
    }

    fn create_wal_load_events(
        filter: &F,
    ) -> (
        VecDeque<IndexSearchEvent>,
        Receiver<TextIndex>,
        Receiver<IndexSearchEvent>,
    ) {
        let mut load_events = VecDeque::new();
        let (tx, rx) = channel();
        let (chained_tx, chained_rx) = channel::<IndexSearchEvent>();

        // Create a shared additive index for incremental merging across chained events
        let shared_additive_index =
            std::sync::Arc::new(std::sync::Mutex::new(AdditiveTextIndex::default()));

        // Determine which attributes to load
        tracing::info!(
            "🔍 WALFinder: Filter attribute resolution - filter.attribute() = {:?}",
            filter.attribute()
        );

        let attributes_to_load = if let Some(attr) = filter.attribute() {
            // If filter specifies an attribute, load only that one
            vec![attr]
        } else {
            vec![]
        };

        // Create load events for each attribute with chained loading support
        tracing::info!(
            "🔍 Creating load events for {} attributes: {:?}",
            attributes_to_load.len(),
            attributes_to_load
        );
        for attr in attributes_to_load {
            let wal_file_name = format!("text_val_Attribute[{}]", attr.0).into_boxed_str();
            trace!(
                "WALFinder creating LoadEvent for {} (attr={})",
                wal_file_name, attr.0
            );

            let tx_clone = tx.clone();
            let base_name = wal_file_name.clone();
            let chained_tx = chained_tx.clone();
            let shared_additive_index = shared_additive_index.clone();
            let load_event = LoadEvent {
                name: wal_file_name,
                send: Box::new(move |_serdes, data| {
                    // Log the exact timestamp being processed for debugging
                    tracing::info!(
                        "🕐 PROCESSING WAL BATCH: {} at timestamp {}",
                        base_name,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    );

                    // Extract metadata from the response data
                    let (data_without_metadata, metadata) = WALFormat::extract_metadata(&data);

                    // Parse the loaded WAL JSON data into WAL entries
                    let wal_entries: Vec<WALEntry> =
                        match WALFormat::from_json(&data_without_metadata) {
                            Ok(entries) => entries,
                            Err(e) => {
                                tracing::error!("Failed to deserialize WAL data: {}", e);
                                return Err(Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    e,
                                )));
                            }
                        };

                    // Convert WAL entries to additive index and merge with existing state
                    let new_additive_index = AdditiveTextIndex::from_wal_entries(&wal_entries);
                    if let Ok(mut index) = shared_additive_index.lock() {
                        index.merge(&new_additive_index);
                    }
                    // Build hierarchical index from merged state
                    let mut text_index = if let Ok(index) = shared_additive_index.lock() {
                        index.to_hierarchical()
                    } else {
                        return Err(Box::new(std::io::Error::other(
                            "Failed to lock shared index",
                        )));
                    };

                    // Calculate comprehensive index statistics
                    let occurrence_count = text_index.occurrences_mut().len();
                    let trigram_count = text_index
                        .trigrams_mut()
                        .values()
                        .map(|positions| positions.len())
                        .sum::<usize>();
                    let token_count = text_index.tokens_mut().len();
                    let unique_trigram_count = text_index.trigrams_mut().len();

                    tracing::info!("📊 INDEX STATISTICS for text attribute {}:", attr.0);
                    tracing::info!("  • Documents/Entries: {}", occurrence_count);
                    tracing::info!("  • Unique Tokens: {}", token_count);
                    tracing::info!("  • Unique Trigrams: {}", unique_trigram_count);
                    tracing::info!("  • Total Trigram Positions: {}", trigram_count);

                    // Handle chaining or send final result
                    match metadata.as_ref().and_then(|m| m.next_timestamp.as_ref()) {
                        Some(timestamp) if timestamp != "EOF" => {
                            // Chain to next batch
                            let next_request_name = format!("{base_name}_{timestamp}");
                            let next_load_event = create_chained_load_event(
                                next_request_name,
                                shared_additive_index.clone(),
                                tx_clone.clone(),
                                chained_tx.clone(),
                            );
                            chained_tx
                                .send(IndexSearchEvent::Load(next_load_event))
                                .ok();
                        }
                        _ => {
                            // Final batch or no chaining - send result
                            tx_clone.send(text_index).ok();
                        }
                    }

                    Ok(())
                }),
            };
            load_events.push_back(IndexSearchEvent::Load(load_event));
        }

        (load_events, rx, chained_rx)
    }
}

impl<F> Iterator for WALFinder<F>
where
    F: TextFilterSansIo,
{
    type Item = IndexSearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            break match std::mem::take(self) {
                WALFinder::Done => None,
                WALFinder::Iterating { mut results } => {
                    let next = results.pop_front();
                    *self = WALFinder::Iterating { results };
                    next
                }
                WALFinder::LoadingWAL {
                    filter,
                    mut loading_receiver,
                    mut load_events,
                    mut chained_receiver,
                    store_ref,
                } => {
                    // Check for any chained events that were sent via the channel
                    if let Some(ref mut chained_rx) = chained_receiver
                        && let Ok(chained_event) = chained_rx.try_recv()
                    {
                        tracing::info!(
                            "🔗 Chained loading protocol: Processing chained LoadEvent: {:?}",
                            chained_event
                        );
                        *self = Self::LoadingWAL {
                            filter,
                            loading_receiver,
                            load_events,
                            chained_receiver,
                            store_ref,
                        };
                        return Some(chained_event);
                    }

                    // Check if we have a built index ready
                    if let Some(ref mut rx) = loading_receiver
                        && let Ok(built_index) = rx.try_recv()
                    {
                        // Store the built index in the persistent cache for future queries
                        if let Ok(store) = store_ref.lock() {
                            store.store_built_index(built_index.clone());
                        }

                        *self = Self::Iterating {
                            results: filter.get(&built_index).collect(),
                        };
                        continue;
                    }

                    // Check if we have more load events to emit
                    if let Some(load_event) = load_events.pop_front() {
                        *self = Self::LoadingWAL {
                            filter,
                            loading_receiver,
                            load_events,
                            chained_receiver,
                            store_ref,
                        };
                        Some(load_event)
                    } else {
                        // No more load events and no built index yet - keep waiting
                        *self = Self::LoadingWAL {
                            filter,
                            loading_receiver,
                            load_events,
                            chained_receiver,
                            store_ref,
                        };
                        None
                    }
                }
            };
        }
    }
}
