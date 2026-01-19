//! Foundation Search engine implementation
//!
//! This module provides `FoundationSearchEngine` which wraps the
//! `proton-foundation-search` crate for local body and metadata text search.
//!
//! ## Atomicity and Transaction Guarantees
//!
//! The `proton-foundation-search` engine uses an interactive commit model where
//! operations are processed through an event iterator. Understanding the atomicity
//! guarantees is important for data integrity:
//!
//! ### Commit Process
//!
//! The engine's commit process works as follows:
//! 1. **Load Phase**: Engine requests blobs via `LoadEvent` - we load from storage and send back
//! 2. **Save Phase**: Engine provides new/modified blobs via `SaveEvent` - we save to storage
//! 3. **Manifest Update**: Engine updates its internal manifest after all Save events complete
//!
//! **Important**: All Save operations are executed atomically in a single database transaction.
//! This ensures that either all blobs are saved or none are, preventing orphaned blobs.
//! The engine's manifest is updated atomically AFTER all blob saves succeed in the transaction.
//! This means:
//! - **Indexing/Removal**: If any Save operation fails, the transaction is rolled back and
//!   the manifest is not updated, so no blobs are orphaned.
//! - **Partial failures**: If the commit iterator fails, no Save operations are executed,
//!   so no inconsistent state occurs.
//!
//! ### Cleanup Process
//!
//! Cleanup is **not fully atomic** - it processes multiple `Release` events sequentially:
//! - If cleanup fails partway through, some blobs may be deleted while others remain
//! - The engine tracks which blobs should be released in its manifest
//! - Failed deletions are logged but don't stop the cleanup process
//! - The engine will retry releasing the same blobs on the next cleanup call
//!
//! **Recovery**: The engine's manifest is the source of truth. If cleanup fails partway,
//! the next cleanup call will attempt to release the same blobs again, ensuring eventual
//! consistency.

use proton_foundation_search::document::{Document, Value};
use proton_foundation_search::engine::{CleanupEvent, Engine, QueryEvent, WriteEvent};
use proton_foundation_search::index::text::TextIndexSansIo;
use proton_foundation_search::processor::ProcessorConfig;
use proton_foundation_search::query::option::QueryOptions;
use proton_foundation_search::query::results::FoundEntry;
use proton_foundation_search::query::stats::CollectionStats;
use proton_foundation_search::serialization::SerDes;
use proton_foundation_search::transaction::{LoadEvent, SaveEvent};
use tracing::{debug, error, info, warn};

use crate::engine::{CleanupResult, IndexResult, SearchStats};
use crate::error::SearchError;
use crate::traits::BlobStorage;
use proton_task_service::{IntoNonPausableFuture, TaskService};
use std::sync::Arc;

/// Extract a human-readable message from a panic payload
fn panic_payload_to_string(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Guard that ensures a blocking task completes before being dropped
///
/// This guard wraps a `JoinHandle` from `spawn_blocking` and ensures that even if
/// the guard (and the future containing it) is dropped, the blocking thread will
/// still complete and release any locks it holds.
///
/// This prevents the race condition where:
/// 1. A future containing `index_message_body()` is dropped
/// 2. The service-level `RwLock` guard is released
/// 3. But the `spawn_blocking` thread continues running with the engine's internal lock held
/// 4. A subsequent operation can acquire the service-level lock but fails on the engine's internal lock
struct BlockingTaskGuard<T: Send + 'static> {
    handle: Option<tokio::task::JoinHandle<Result<T, SearchError>>>,
}

impl<T: Send + 'static> BlockingTaskGuard<T> {
    /// Create a new guard wrapping a `JoinHandle`
    fn new(handle: tokio::task::JoinHandle<Result<T, SearchError>>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    /// Await the blocking task and return its result
    ///
    /// This must be called to get the result. If the guard is dropped without
    /// being awaited, the `Drop` implementation will ensure the blocking thread
    /// completes in the background.
    async fn wait(mut self) -> Result<T, SearchError> {
        let handle = self
            .handle
            .take()
            .expect("BlockingTaskGuard::await() called twice");
        match handle.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e),
            Err(join_error) => {
                let msg = if join_error.is_panic() {
                    panic_payload_to_string(&join_error.into_panic())
                } else {
                    "Task cancelled".to_string()
                };
                Err(SearchError::Panic(format!(
                    "Commit iterator task failed: {msg}"
                )))
            }
        }
    }
}

impl<T: Send + 'static> Drop for BlockingTaskGuard<T> {
    fn drop(&mut self) {
        // If the guard is dropped without being awaited, spawn a background task
        // to await the blocking thread. This ensures the blocking thread completes
        // and releases the engine's internal lock, even if the original future is dropped.
        if let Some(join_handle) = self.handle.take() {
            // Use Handle::try_current() to ensure we can spawn even if we're not in a tokio context
            // This is safe because we're just ensuring the blocking thread completes
            if let Ok(runtime_handle) = tokio::runtime::Handle::try_current() {
                // Spawn a task to await the blocking thread
                // The join_handle from spawn_blocking is Send, so this async block is Send
                runtime_handle.spawn(async move {
                    let _ = join_handle.await;
                });
            } else {
                // If we're not in a tokio context, we can't spawn, but that's okay
                // The blocking thread will complete on its own
                // Note: In practice, this code always runs in a tokio context
            }
        }
    }
}

/// Field names for the search index
pub mod field {
    /// Body text field - the decrypted message content
    pub const BODY: &str = "body";
    /// Subject field - the message subject line
    pub const SUBJECT: &str = "subject";
    /// From field - the sender email address
    pub const FROM: &str = "from";
    /// To field - the primary recipients
    pub const TO: &str = "to";
    /// CC field - the CC recipients
    pub const CC: &str = "cc";
    /// BCC field - the BCC recipients
    pub const BCC: &str = "bcc";
}

/// Foundation Search engine implementation
///
/// Wraps the Foundation Search engine and provides blob storage via
/// the `BlobStorage` trait.
pub struct FoundationSearchEngine<S: BlobStorage> {
    /// The Foundation Search engine
    engine: Engine,
    /// Serialization format for index blobs
    serdes: SerDes,
    /// Blob storage backend
    storage: S,
    /// Task service for spawning pausable background tasks
    task_service: Arc<TaskService>,
}

impl<S: BlobStorage + Clone + 'static> FoundationSearchEngine<S> {
    /// Create a new Foundation Search engine with the given storage backend
    ///
    /// The `task_service` is used to spawn background tasks that can be paused
    /// when the app goes into the background.
    pub fn new(storage: S, task_service: Arc<TaskService>) -> Self {
        info!("Initializing Foundation Search engine");

        let engine = Engine::builder()
            .with_builtin_processor(ProcessorConfig::default())
            .with_index(TextIndexSansIo::default())
            .build();

        Self {
            engine,
            serdes: SerDes::Cbor,
            storage,
            task_service,
        }
    }
}

impl<S: BlobStorage + Clone + 'static> FoundationSearchEngine<S> {
    /// Build a document for message body and metadata
    ///
    /// Creates a document with body text and metadata fields (subject, from, to, cc, bcc)
    /// for comprehensive offline search capabilities.
    ///
    /// Metadata is required to ensure complete indexing - all messages must have
    /// metadata (subject, from, to, etc.) for proper search functionality.
    fn build_document(
        message_id: &str,
        body: &str,
        metadata: &crate::traits::MessageMetadata,
    ) -> Document {
        let body_doc_id = format!("{message_id}_body");
        Document::new(&body_doc_id)
            .with_attribute(field::BODY, Value::text(body))
            .with_attribute(field::SUBJECT, Value::text(metadata.subject.as_str()))
            .with_attribute(field::FROM, Value::text(metadata.from.as_str()))
            .with_attribute(field::TO, Value::text(metadata.to.as_str()))
            .with_attribute(field::CC, Value::text(metadata.cc.as_str()))
            .with_attribute(field::BCC, Value::text(metadata.bcc.as_str()))
    }

    /// Process commit iterator using channels to keep I/O async while iterator runs in `spawn_blocking`
    ///
    /// This function processes the commit iterator interactively:
    /// - Iterator runs in `spawn_blocking` (CPU-bound work, must stay on one thread)
    /// - Load events are sent via channel to async task for I/O (doesn't block thread pool)
    /// - Save events are collected for atomic execution in a transaction
    ///
    /// This is more efficient than using `handle.block_on()` inside `spawn_blocking` because
    /// I/O operations run on the async runtime instead of blocking thread pool threads.
    async fn process_commit_iterator_with_async_io<I>(
        &self,
        storage: S,
        serdes: SerDes,
        commit_iter_fn: impl FnOnce() -> Result<I, SearchError> + Send + 'static,
    ) -> Result<BlockingTaskGuard<Vec<(String, Vec<u8>)>>, SearchError>
    where
        I: Iterator<Item = WriteEvent> + Send + 'static,
    {
        use tokio::sync::oneshot;

        let (load_tx, mut load_rx) = tokio::sync::mpsc::unbounded_channel::<(
            String,
            oneshot::Sender<Result<Vec<u8>, SearchError>>,
        )>();
        let storage_clone = storage.clone();

        // Spawn async task to handle Load requests using TaskService
        // This task is marked as non-pausable because it's part of a critical indexing
        // operation that must complete. If it gets paused, the blocking thread will
        // hang waiting for load responses, causing the entire indexing operation to fail.
        let task_service = Arc::clone(&self.task_service);
        let load_handle = task_service.spawn(
            async move {
                while let Some((name, response_tx)) = load_rx.recv().await {
                    let blob_result = storage_clone
                        .load(&name)
                        .await
                        .map_err(|e| {
                            SearchError::BlobStorage(format!("Failed to load '{name}': {e}"))
                        })
                        .map(Option::unwrap_or_default);
                    let _ = response_tx.send(blob_result);
                }
            }
            .into_non_pausable(),
        );

        // Clone load_tx for use in spawn_blocking (we'll drop the original after)
        let load_tx_for_blocking = load_tx.clone();

        // Process iterator in spawn_blocking, but use channel for Load I/O
        let blocking_handle = tokio::task::spawn_blocking(move || {
            let commit_iter = commit_iter_fn()?;
            let mut save_ops = Vec::new();

            for event in commit_iter {
                match event {
                    WriteEvent::Load(LoadEvent { name, send }) => {
                        // Send load request to async task via channel
                        let (tx, rx) = oneshot::channel();
                        if load_tx_for_blocking.send((name.to_string(), tx)).is_err() {
                            return Err(SearchError::Internal("Load channel closed".to_string()));
                        }

                        // Wait for async load to complete (blocking call, but I/O is async)
                        let blob = match rx.blocking_recv() {
                            Ok(Ok(blob)) => blob,
                            Ok(Err(e)) => return Err(e),
                            Err(_) => {
                                return Err(SearchError::Internal(
                                    "Load response channel closed".to_string(),
                                ));
                            }
                        };

                        send(&serdes, blob).map_err(|e| {
                            SearchError::Serialization(format!("Failed to send '{name}': {e:?}"))
                        })?;
                    }
                    WriteEvent::Save(SaveEvent { name, recv }) => {
                        let blob = recv(&serdes).map_err(|e| {
                            SearchError::Serialization(format!("Failed to recv '{name}': {e:?}"))
                        })?;
                        save_ops.push((name.to_string(), blob));
                    }
                    WriteEvent::Modified(_id) => {
                        // Document operation completed successfully
                    }
                }
            }

            Ok::<_, SearchError>(save_ops)
        });

        // Close load channel and wait for load task to finish
        drop(load_tx);
        // Handle task completion or cancellation gracefully
        // If the task was cancelled (e.g., during app shutdown), we should fail the operation
        // rather than panic, as this is an expected condition during cancellation
        if let Err(join_error) = load_handle.await {
            let msg = if join_error.is_panic() {
                panic_payload_to_string(&join_error.into_panic())
            } else {
                // Task was cancelled - this is expected during app lifecycle events
                // Return an error that indicates the operation was cancelled
                return Err(SearchError::Internal(
                    "Indexing operation was cancelled".to_string(),
                ));
            };
            return Err(SearchError::Panic(format!("Load task panicked: {msg}")));
        }

        // Return a guard that ensures the blocking thread completes even if dropped
        Ok(BlockingTaskGuard::new(blocking_handle))
    }

    /// Index a document and commit
    ///
    /// Takes `&mut self` to enforce exclusive access at compile time.
    ///
    /// ## Performance
    ///
    /// The commit iterator runs in `spawn_blocking` (CPU-bound work), but I/O operations
    /// (Load events) are processed asynchronously via channels, preventing thread pool
    /// threads from blocking on I/O.
    ///
    /// ## Atomicity
    ///
    /// The commit process is interactive: we must respond to Load events before Save events
    /// are generated. If any Save event fails, the entire operation fails and the engine's
    /// manifest is not updated, ensuring no orphaned blobs are created.
    ///
    /// The engine updates its manifest atomically after all Save events succeed, so partial
    /// failures result in a complete rollback (no blobs saved, manifest unchanged).
    async fn index_document(&mut self, doc: Document) -> Result<IndexResult, SearchError> {
        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let serdes = self.serdes;

        // Use async I/O processing instead of blocking on I/O inside spawn_blocking
        let save_operations_guard = self
            .process_commit_iterator_with_async_io(
                storage,
                serdes,
                move || {
                    // Writes are serialized by RwLock in service.rs, so this should never fail.
                    // If it does, it indicates a bug in our serialization logic.
                    let mut writer = engine.write().expect("Engine write lock should always succeed - writes are serialized by service-level RwLock");

                    writer.insert(doc).map_err(|e| {
                        SearchError::Internal(format!("Failed to insert document: {e:?}"))
                    })?;

                    Ok(writer.commit())
                },
            )
            .await?;

        // Await the blocking thread to complete and get the save operations
        // This ensures the engine's internal lock is released before we proceed
        let save_operations = save_operations_guard.wait().await?;

        // Execute all Save operations atomically in a transaction
        // This prevents orphaned blobs if the operation fails mid-way
        self.storage.save_batch_atomic(save_operations).await?;
        Ok(IndexResult::needs_cleanup())
    }

    /// Index multiple documents in a single commit transaction
    ///
    /// This is more efficient than calling `index_document` multiple times
    /// because it performs a single commit operation for all documents.
    ///
    /// Takes `&mut self` to enforce exclusive access at compile time.
    ///
    /// ## Performance
    ///
    /// The commit iterator runs in `spawn_blocking` (CPU-bound work), but I/O operations
    /// (Load events) are processed asynchronously via channels, preventing thread pool
    /// threads from blocking on I/O.
    async fn index_documents_batch(
        &mut self,
        docs: Vec<Document>,
    ) -> Result<IndexResult, SearchError> {
        if docs.is_empty() {
            return Ok(IndexResult::no_cleanup());
        }

        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let serdes = self.serdes;

        // Use async I/O processing instead of blocking on I/O inside spawn_blocking
        let save_operations_guard = self
            .process_commit_iterator_with_async_io(
                storage,
                serdes,
                move || {
                    // Writes are serialized by RwLock in service.rs, so this should never fail.
                    // If it does, it indicates a bug in our serialization logic.
                    let mut writer = engine.write().expect("Engine write lock should always succeed - writes are serialized by service-level RwLock");

                    // Insert all documents before committing
                    for doc in docs {
                        writer.insert(doc).map_err(|e| {
                            SearchError::Internal(format!("Failed to insert document: {e:?}"))
                        })?;
                    }

                    Ok(writer.commit())
                },
            )
            .await?;

        // Await the blocking thread to complete and get the save operations
        // This ensures the engine's internal lock is released before we proceed
        let save_operations = save_operations_guard.wait().await?;

        // Execute all Save operations atomically in a transaction
        // This prevents orphaned blobs if the operation fails mid-way
        self.storage.save_batch_atomic(save_operations).await?;
        Ok(IndexResult::needs_cleanup())
    }

    /// Remove documents and commit
    ///
    /// Safe to call even if documents don't exist - the operation will complete successfully.
    ///
    /// ## Performance
    ///
    /// The commit iterator runs in `spawn_blocking` (CPU-bound work), but I/O operations
    /// (Load events) are processed asynchronously via channels, preventing thread pool
    /// threads from blocking on I/O.
    ///
    /// ## Atomicity
    ///
    /// Similar to indexing, the commit process processes Load/Save events interactively.
    /// If any Save event fails, the entire operation fails and the manifest is not updated,
    /// ensuring the index remains in a consistent state.
    ///
    /// The engine handles manifest updates atomically after all Save events succeed, so
    /// partial failures result in a complete rollback.
    async fn remove_documents(&mut self, doc_ids: &[&str]) -> Result<IndexResult, SearchError> {
        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let serdes = self.serdes;
        let doc_ids_owned: Vec<String> = doc_ids.iter().map(|s| (*s).to_string()).collect();

        // Use async I/O processing instead of blocking on I/O inside spawn_blocking
        let save_operations_guard = self
            .process_commit_iterator_with_async_io(
                storage,
                serdes,
                move || {
                    // Writes are serialized by RwLock in service.rs, so this should never fail.
                    // If it does, it indicates a bug in our serialization logic.
                    let mut writer = engine.write().expect("Engine write lock should always succeed - writes are serialized by service-level RwLock");

                    for doc_id in &doc_ids_owned {
                        writer.remove(doc_id);
                    }

                    Ok(writer.commit())
                },
            )
            .await?;

        // Await the blocking thread to complete and get the save operations
        // This ensures the engine's internal lock is released before we proceed
        let save_operations = save_operations_guard.wait().await?;

        // Execute all Save operations atomically in a transaction
        // This prevents orphaned blobs if the operation fails mid-way
        self.storage.save_batch_atomic(save_operations).await?;
        Ok(IndexResult::needs_cleanup())
    }

    // --- Public API ---

    /// Index a message's body content and metadata
    ///
    /// This indexes the decrypted message body and metadata (subject, from, to, cc, bcc)
    /// for comprehensive offline full-text search.
    ///
    /// Takes `&mut self` to enforce exclusive access - concurrent indexing is not supported.
    pub async fn index_message(
        &mut self,
        message_id: &str,
        body: &str,
        metadata: &crate::traits::MessageMetadata,
    ) -> Result<IndexResult, SearchError> {
        debug!("Indexing body and metadata for message {:?}", message_id);
        let doc = Self::build_document(message_id, body, metadata);
        self.index_document(doc).await.map_err(|e| {
            warn!("Failed to index message {}: {}", message_id, e);
            e
        })
    }

    /// Index multiple message bodies and metadata in a single commit transaction
    ///
    /// This is more efficient than calling `index_message` multiple times
    /// because it performs a single commit operation for all messages.
    ///
    /// Takes `&mut self` to enforce exclusive access.
    pub async fn index_bodies_batch(
        &mut self,
        messages: &[(&str, &str, &crate::traits::MessageMetadata)],
    ) -> Result<IndexResult, SearchError> {
        if messages.is_empty() {
            return Ok(IndexResult::no_cleanup());
        }

        debug!(
            "Batch indexing {} message bodies with metadata",
            messages.len()
        );
        let docs: Vec<Document> = messages
            .iter()
            .map(|(message_id, body, metadata)| Self::build_document(message_id, body, metadata))
            .collect();

        self.index_documents_batch(docs).await.map_err(|e| {
            warn!(
                "Failed to batch index {} message bodies: {}",
                messages.len(),
                e
            );
            e
        })
    }

    /// Remove a message from the index
    ///
    /// Safe to call even if the message was never indexed.
    ///
    /// Takes `&mut self` to enforce exclusive access.
    pub async fn remove_message(&mut self, message_id: &str) -> Result<IndexResult, SearchError> {
        debug!("Removing message {:?} from search index", message_id);

        let metadata_doc_id = message_id.to_string();
        let body_doc_id = format!("{message_id}_body");

        self.remove_documents(&[&metadata_doc_id, &body_doc_id])
            .await
    }

    /// Run cleanup to delete obsolete blobs
    ///
    /// Should be called periodically after write operations.
    ///
    /// Takes `&mut self` to enforce exclusive access.
    ///
    /// ## Atomicity and Error Handling
    ///
    /// Cleanup processes multiple `Release` events sequentially and is **not fully atomic**.
    /// If cleanup fails partway through:
    /// - Some blobs may be deleted while others remain
    /// - Failed deletions are logged as warnings but don't stop the process
    /// - The engine tracks which blobs should be released in its manifest
    /// - The next cleanup call will retry releasing the same blobs, ensuring eventual consistency
    ///
    /// Storage failures during blob deletion are handled gracefully: we log a warning and
    /// continue processing remaining blobs. This ensures cleanup makes progress even if some
    /// deletions fail (e.g., due to transient storage issues).
    ///
    /// Load/Save events during cleanup follow the same interactive pattern as indexing:
    /// if a Save event fails, cleanup fails entirely and no blobs are deleted.
    pub async fn cleanup(&mut self) -> Result<CleanupResult, SearchError> {
        debug!("FoundationSearchEngine::cleanup() called");

        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let serdes = self.serdes;

        // IMPORTANT: The cleanup iterator is interactive - we must respond to
        // Load events DURING iteration before it yields Release events.
        let result = tokio::task::spawn_blocking(move || {
            // Cleanup is serialized by RwLock in service.rs, so this should never fail.
            // If it does, it indicates a bug in our serialization logic.
            let cleanup = engine.cleanup().expect("Engine cleanup should always succeed - operations are serialized by service-level RwLock");

            let handle = tokio::runtime::Handle::current();
            let mut deleted_count = 0;

            for event in cleanup {
                match event {
                    CleanupEvent::Load(LoadEvent { name, send }) => {
                        debug!("Cleanup: loading blob '{}'", name);
                        let blob = handle
                            .block_on(storage.load(&name))
                            .map_err(|e| {
                                SearchError::BlobStorage(format!(
                                    "Failed to load '{name}': {e}"
                                ))
                            })?
                            .unwrap_or_default();
                        send(&serdes, blob).map_err(|e| {
                            SearchError::Serialization(format!(
                                "Failed to send '{name}': {e:?}"
                            ))
                        })?;
                    }
                    CleanupEvent::Save(SaveEvent { name, recv }) => {
                        debug!("Cleanup: saving blob '{}'", name);
                        let blob = recv(&serdes).map_err(|e| {
                            SearchError::Serialization(format!(
                                "Failed to recv '{name}': {e:?}"
                            ))
                        })?;
                        handle.block_on(storage.save(&name, &blob)).map_err(|e| {
                            SearchError::BlobStorage(format!("Failed to save '{name}': {e}"))
                        })?;
                    }
                    CleanupEvent::Release(blob_name) => {
                        let name = blob_name.as_ref();
                        debug!("Cleanup: releasing blob '{}'", name);
                        match handle.block_on(storage.delete(name)) {
                            Ok(true) => {
                                deleted_count += 1;
                                debug!("Cleanup: deleted blob '{}'", name);
                            }
                            Ok(false) => {
                                debug!("Cleanup: blob '{}' not found (already deleted?)", name);
                                // Not an error - blob may have been deleted by another process
                                // or in a previous cleanup attempt
                            }
                            Err(e) => {
                                // Log but continue - allows cleanup to make progress even if
                                // some deletions fail. The engine will retry releasing this blob
                                // on the next cleanup call.
                                warn!(
                                    "Cleanup: failed to delete blob '{}': {}. Will retry on next cleanup.",
                                    name, e
                                );
                            }
                        }
                    }
                }
            }

            info!("Cleanup completed: {} blobs deleted", deleted_count);
            Ok(deleted_count)
        })
        .await;

        match result {
            Ok(Ok(deleted_count)) => Ok(CleanupResult {
                blobs_deleted: deleted_count,
            }),
            Ok(Err(e)) => Err(e),
            Err(join_error) => {
                let msg = if join_error.is_panic() {
                    panic_payload_to_string(&join_error.into_panic())
                } else {
                    "Task cancelled".to_string()
                };
                error!("Foundation Search task failed during cleanup: {}", msg);
                Err(SearchError::Panic(msg))
            }
        }
    }

    /// Get index statistics
    pub fn stats(&self) -> SearchStats {
        let stats = self.engine.stats();
        SearchStats {
            documents_total: stats.documents_total.unwrap_or(0),
            is_writing: stats.writing,
        }
    }

    /// Check if index has any documents
    pub fn has_documents(&self) -> bool {
        self.stats().documents_total > 0
    }

    /// Clear all index data
    ///
    /// Removes all stored index blobs, effectively resetting the search index.
    /// The engine will start fresh on the next indexing operation.
    ///
    /// Takes `&mut self` to enforce exclusive access.
    pub async fn clear(&mut self) -> Result<(), SearchError> {
        info!("Clearing all search index data");

        // Clear all blobs from storage
        self.storage.clear_all().await?;

        info!("Successfully cleared all search index data");
        Ok(())
    }

    // --- Search Methods ---

    /// Search and return raw `FoundEntry` objects for full access to match metadata
    ///
    /// This method provides direct access to Foundation Search's `FoundEntry`,
    /// which includes document IDs, scores, and match positions for highlighting.
    /// Use this when you need full access to match positions for UI highlighting.
    pub async fn search_raw(&self, query: &str) -> Result<Vec<FoundEntry>, SearchError> {
        debug!("Searching local index (raw): {}", query);

        let expression = query.parse().map_err(|e| {
            SearchError::InvalidQuery(format!("Failed to parse query '{query}': {e:?}"))
        })?;

        let options = QueryOptions::default();

        let search_query = self
            .engine
            .query()
            .with_expression(expression)
            .with_options(options)
            .search();

        let mut results: Vec<FoundEntry> = Vec::new();
        let mut stats = CollectionStats::default();

        for event in search_query {
            match event {
                QueryEvent::Load(LoadEvent { name, send }) => {
                    let blob = self.storage.load(&name).await?.unwrap_or_default();
                    send(&self.serdes, blob).map_err(|e| {
                        SearchError::Serialization(format!("Failed to send blob '{name}': {e:?}"))
                    })?;
                }
                QueryEvent::Found(found) => {
                    results.push(found);
                }
                QueryEvent::Stats(query_stats) => {
                    // Accumulate collection statistics for TF-IDF score calculation
                    stats += query_stats;
                }
            }
        }

        // Apply TF-IDF scores using the collected statistics
        // This ensures proper relevance ranking based on term frequency and inverse document frequency
        for result in &mut results {
            stats.update_scores(result);
        }

        // Sort by relevance score (descending), then by identifier for stability
        // Use explicit sorting key to avoid PartialEq/PartialOrd trait inconsistency until fixed
        results.sort_by(|a, b| {
            let score_a = a.score();
            let score_b = b.score();
            let id_a = a.identifier();
            let id_b = b.identifier();

            // Compare by score first (descending), then by identifier (ascending) for stability
            match score_b.cmp(&score_a) {
                std::cmp::Ordering::Equal => id_a.cmp(id_b),
                other => other,
            }
        });

        debug!("Found {} local results", results.len());
        Ok(results)
    }

    /// Search and return results with full metadata for UI highlighting
    ///
    /// This is an alias for `search_raw()` that provides access to `FoundEntry`
    /// objects containing match positions for highlighting.
    pub async fn search_with_metadata(&self, query: &str) -> Result<Vec<FoundEntry>, SearchError> {
        self.search_raw(query).await
    }
}
