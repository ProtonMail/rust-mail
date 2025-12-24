//! Foundation Search service for indexing and searching emails locally
//!
//! This service provides local search for both message bodies and metadata (subject, from, to, etc.).
//! All search is performed locally on decrypted content for privacy and offline capability.
//!
//! Key features:
//! - Local trigram-based full-text search of message bodies
//! - Persistent storage of index blobs in SQLite
//! - Privacy-preserving design (only decrypted on device)

use std::sync::Arc;

use proton_mail_api::services::proton::common::MessageId;
use stash::stash::{Bond, Stash, StashError};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::SearchError;
use crate::foundation::FoundationSearchEngine;
use crate::intent::{LocalMessageId, SearchIndexIntent, SearchOperation};
use crate::storage::StashBlobStorage;
use crate::traits::MessageDataProvider;
use crate::worker::SearchIndexWorker;

/// Error type for search service operations
#[derive(Debug, thiserror::Error)]
pub enum SearchServiceError {
    /// Error from the search engine
    #[error("Search engine error: {0}")]
    Engine(#[from] SearchError),

    /// Error during cleanup
    #[error("Cleanup failed: {0}")]
    Cleanup(SearchError),

    /// Error clearing the index
    #[error("Clear failed: {0}")]
    Clear(SearchError),
}

impl SearchServiceError {
    pub fn into_inner(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }
}

/// The Foundation Search service for local email search
///
/// This service wraps the search engine and handles:
/// - Body text indexing for full-text search
/// - Local search queries
///
/// The engine is wrapped in `RwLock` because write operations (`index_body`,
/// `remove_message`, `cleanup`, `clear`) require exclusive access, while
/// read operations (`search_with_metadata`, `stats`) can run concurrently.
#[derive(Clone)]
pub struct MailSearchService {
    engine: Arc<RwLock<FoundationSearchEngine<StashBlobStorage>>>,
    stash: Stash,
}

impl MailSearchService {
    /// Create a new search service with a Stash database connection pool
    ///
    /// The service creates a Foundation Search engine with:
    /// - Text index for trigram-based full-text search
    /// - Built-in processor for tokenization
    /// - Persistent blob storage via Stash
    pub fn new(stash: Stash) -> Self {
        info!("Initializing Foundation Search engine with Stash");

        let storage = StashBlobStorage::new(stash.clone());
        let engine = FoundationSearchEngine::new_with_stash(storage);

        Self {
            engine: Arc::new(RwLock::new(engine)),
            stash,
        }
    }

    /// Get a reference to the underlying Stash connection pool
    pub fn stash(&self) -> &Stash {
        &self.stash
    }

    /// Index or update message body content and metadata
    ///
    /// This should be called after a message body is decrypted and stored.
    /// It creates a document with body text and metadata (subject, from, to, cc, bcc)
    /// for comprehensive offline full-text search.
    ///
    /// Uses the remote `MessageId` (not local ID) so indices can be merged
    /// across devices if needed in the future.
    pub async fn index_message_body(
        &self,
        remote_id: &MessageId,
        body: &str,
        metadata: &crate::traits::MessageMetadata,
    ) -> Result<(), SearchServiceError> {
        let doc_id = remote_id.as_str();

        let result = self
            .engine
            .write()
            .await
            .index_body(doc_id, body, metadata)
            .await?;

        if result.cleanup_needed
            && let Err(e) = self.cleanup().await
        {
            warn!("Automatic cleanup after indexing failed: {}", e);
            // Don't fail the operation if cleanup fails - it can be retried later
        }

        Ok(())
    }

    /// Index multiple message bodies and metadata in a single commit transaction
    ///
    /// This is more efficient than calling `index_message_body` multiple times
    /// because it performs a single commit operation for all messages.
    ///
    /// Uses the remote `MessageId` (not local ID) so indices can be merged
    /// across devices if needed in the future.
    pub async fn index_message_bodies_batch(
        &self,
        messages: &[(&MessageId, &str, &crate::traits::MessageMetadata)],
    ) -> Result<(), SearchServiceError> {
        if messages.is_empty() {
            return Ok(());
        }

        let message_refs: Vec<(&str, &str, &crate::traits::MessageMetadata)> = messages
            .iter()
            .map(|(remote_id, body, metadata)| (remote_id.as_str(), *body, *metadata))
            .collect();

        let result = self
            .engine
            .write()
            .await
            .index_bodies_batch(&message_refs)
            .await?;

        if result.cleanup_needed
            && let Err(e) = self.cleanup().await
        {
            warn!("Automatic cleanup after batch indexing failed: {}", e);
            // Don't fail the operation if cleanup fails - it can be retried later
        }

        Ok(())
    }

    /// Remove a message from the search index
    ///
    /// This removes the body document from the search index.
    /// Should be called when a message is deleted.
    ///
    /// Uses the remote `MessageId` (not local ID) for consistency with indexing.
    pub async fn remove_message(&self, remote_id: &MessageId) -> Result<(), SearchServiceError> {
        debug!("Removing message {:?} from search index", remote_id);

        let result = self
            .engine
            .write()
            .await
            .remove_message(remote_id.as_str())
            .await?;

        if result.cleanup_needed
            && let Err(e) = self.cleanup().await
        {
            warn!("Automatic cleanup after removal failed: {}", e);
            // Don't fail the operation if cleanup fails - it can be retried later
        }

        Ok(())
    }

    /// Search for messages and return full metadata including match positions for highlighting.
    ///
    /// This is an enhanced version that returns `FoundEntry` objects
    /// containing position metadata for highlighting search terms in the UI.
    ///
    /// Returns results sorted by relevance score (descending).
    pub async fn search_local_with_metadata(
        &self,
        query: &str,
    ) -> Result<Vec<proton_foundation_search::query::results::FoundEntry>, SearchServiceError> {
        debug!("Searching local index with metadata: {}", query);

        let results = self.engine.read().await.search_with_metadata(query).await?;

        debug!("Found {} local results with metadata", results.len());
        Ok(results)
    }

    /// Check if local index has any documents
    pub async fn has_indexed_documents(&self) -> bool {
        self.engine.read().await.stats().documents_total > 0
    }

    /// Get index statistics
    pub async fn get_stats(&self) -> IndexStats {
        let stats = self.engine.read().await.stats();
        IndexStats {
            documents_total: stats.documents_total,
            is_writing: stats.is_writing,
        }
    }

    /// Clear all index data (for debugging/testing)
    #[allow(dead_code)]
    pub async fn clear_index(&self) -> Result<(), SearchServiceError> {
        self.engine
            .write()
            .await
            .clear()
            .await
            .map_err(SearchServiceError::Clear)?;

        info!("Cleared all search index data");
        Ok(())
    }

    /// Clean up obsolete index blobs
    ///
    /// Cleanup is automatically called after write/delete operations when needed.
    /// This method is still available for manual cleanup or periodic maintenance
    /// in long-running applications.
    ///
    /// Returns the number of blobs deleted.
    pub async fn cleanup(&self) -> Result<usize, SearchServiceError> {
        debug!("MailSearchService::cleanup() called");

        let result = self
            .engine
            .write()
            .await
            .cleanup()
            .await
            .map_err(SearchServiceError::Cleanup)?;

        debug!("Cleanup result: {} blobs deleted", result.blobs_deleted);

        Ok(result.blobs_deleted)
    }

    // --- Intent Queue API ---
    //
    // These methods create search intents within a database transaction.
    // The actual indexing/removal is performed asynchronously by SearchIndexWorker.

    /// Queue a message for search indexing
    ///
    /// Creates an intent to index the message body. The actual indexing is
    /// performed asynchronously by the background worker.
    ///
    /// This should be called within a transaction (e.g., when storing message body)
    /// to ensure atomicity - if the transaction rolls back, the intent is also rolled back.
    ///
    /// The worker is automatically notified via database table watcher after the
    /// transaction commits, eliminating race conditions and supporting multi-account scenarios.
    pub async fn queue_index(
        message_id: LocalMessageId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        // Create intent (content hash check happens in worker using separate table)
        // The table watcher will automatically notify the worker after commit
        SearchIndexIntent::create_or_ignore(message_id, SearchOperation::Index, bond).await?;

        Ok(())
    }

    /// Queue multiple messages for search indexing in a single batch operation
    ///
    /// Creates intents to index multiple message bodies. The actual indexing is
    /// performed asynchronously by the background worker.
    ///
    /// This should be called within a transaction (e.g., when storing multiple message bodies)
    /// to ensure atomicity - if the transaction rolls back, all intents are also rolled back.
    ///
    /// This is more efficient than calling `queue_index` multiple times because it
    /// performs a single SQL statement for all intents.
    ///
    /// The worker is automatically notified via database table watcher after the
    /// transaction commits, eliminating race conditions and supporting multi-account scenarios.
    pub async fn queue_index_batch(
        message_ids: &[LocalMessageId],
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        if message_ids.is_empty() {
            return Ok(());
        }

        SearchIndexIntent::create_or_ignore_batch(message_ids, SearchOperation::Index, bond)
            .await?;
        // Table watcher will automatically notify the worker after commit
        Ok(())
    }

    /// Queue a message for removal from search index
    ///
    /// Creates an intent to remove the message from the index. The actual removal
    /// is performed asynchronously by the background worker.
    ///
    /// This should be called within a transaction (e.g., when deleting a message)
    /// to ensure atomicity.
    ///
    /// The worker is automatically notified via database table watcher after the
    /// transaction commits, eliminating race conditions and supporting multi-account scenarios.
    pub async fn queue_remove(
        message_id: LocalMessageId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        // Remove operations don't need content hash (always remove regardless of content)
        // The table watcher will automatically notify the worker after commit
        SearchIndexIntent::create_or_ignore(message_id, SearchOperation::Remove, bond).await?;

        Ok(())
    }

    /// Create and initialize the search index worker that processes pending index intents
    ///
    /// The worker runs in the background and:
    /// - Waits for new intents via database table watcher (no polling, no race conditions)
    /// - Executes indexing/removal operations via `MailSearchService`
    /// - Handles retries with exponential backoff
    /// - Processes cleanup when the queue is empty
    ///
    /// The worker uses a database table watcher that automatically detects changes
    /// to the `search_index_intents` table after transactions commit, ensuring:
    /// - No race conditions (notification happens after commit)
    /// - Multi-account support (each Stash instance has its own watcher)
    pub async fn create_worker<P: MessageDataProvider>(
        &self,
        data_provider: Arc<P>,
    ) -> Result<SearchIndexWorker<P>, StashError> {
        info!("Creating search index worker with table watcher");

        // Create table watcher for this Stash instance (account-specific)
        let watcher_handle = crate::watcher::SearchIndexIntentWatcher::watch(&self.stash).await?;

        Ok(SearchIndexWorker::new(
            self.stash.clone(),
            self.clone(),
            data_provider,
            watcher_handle,
        ))
    }
}

/// Statistics about the search index
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Total number of documents in the index
    pub documents_total: usize,
    /// Whether the engine is currently writing
    pub is_writing: bool,
}
