//! Self-contained search engine for Proton Mail local search
//!
//! This crate provides local search for both message bodies and metadata (subject, from, to, etc.).
//! All search is performed locally on decrypted content for privacy and offline capability.
//!
//! # Architecture (Split MR Design)
//!
//! This crate is self-contained with:
//! - **Intent model** (`SearchIndexIntent`) - Transactional Outbox pattern for crash safety
//! - **Service** (`MailSearchService`) - High-level API for indexing and searching
//! - **Worker** (`SearchIndexWorker`) - Background processing of intents
//! - **Storage** (`StashBlobStorage`) - SQLite-based blob storage
//! - **Engine** (`FoundationSearchEngine`) - Wrapper around Foundation Search
//!
//! The only external dependency is `MessageDataProvider` trait, implemented by mail-common
//! to provide message body and remote ID data.
//!
//! # Feature Flags
//!
//! The `foundation_search` feature flag is used to conditionally compile the Foundation Search
//! implementation. This allows the crate to be used in contexts where the foundation search
//! dependency is not available. In practice, this feature is always enabled for the mail app.
//!
//! # Example
//!
//! ```ignore
//! use proton_mail_search::{MailSearchService, SearchIndexWorker, MessageDataProvider};
//!
//! // Create service with database connection
//! let service = MailSearchService::new(stash);
//!
//! // Create worker with message data provider
//! let worker = SearchIndexWorker::new(stash, service.clone(), data_provider);
//!
//! // Spawn worker in background
//! tokio::spawn(async move { worker.run().await });
//!
//! // Queue messages for indexing (in a transaction)
//! MailSearchService::queue_index(message_id, &bond).await?;
//!
//! // Search
//! let results = service.search_local_with_metadata("hello").await?;
//! ```

mod engine;
mod error;

// Migrations (always available)
pub mod migrations;

// Intent model (always available)
pub mod intent;

// External traits (always available)
pub mod traits;

#[cfg(feature = "foundation_search")]
mod foundation;

#[cfg(feature = "foundation_search")]
mod service;

#[cfg(feature = "foundation_search")]
mod storage;

#[cfg(feature = "foundation_search")]
mod watcher;

#[cfg(feature = "foundation_search")]
mod worker;

// Core types (always available)
pub use engine::{CleanupResult, IndexResult, SearchStats};
pub use error::SearchError;
pub use intent::{LocalMessageId, SearchIndexIntent, SearchOperation};
pub use traits::{MessageDataProvider, MessageMetadata};

// Foundation Search types (feature-gated)
#[cfg(feature = "foundation_search")]
pub use foundation::FoundationSearchEngine;

#[cfg(feature = "foundation_search")]
pub use traits::BlobStorage;

#[cfg(feature = "foundation_search")]
pub use service::{IndexStats, MailSearchService, SearchServiceError};

#[cfg(feature = "foundation_search")]
pub use storage::StashBlobStorage;

#[cfg(feature = "foundation_search")]
pub use watcher::SearchIndexIntentWatcher;

#[cfg(feature = "foundation_search")]
pub use worker::SearchIndexWorker;

#[cfg(feature = "foundation_search")]
pub use proton_foundation_search::query::results::FoundEntry;

#[cfg(test)]
#[cfg(feature = "foundation_search")]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    /// In-memory blob storage for testing
    ///
    /// Uses Arc<RwLock<...>> so clones share the same underlying data.
    /// This is important because FoundationSearchEngine clones storage
    /// when spawning blocking tasks.
    /// RwLock allows concurrent reads, which is more appropriate for test storage.
    #[derive(Clone)]
    struct InMemoryBlobStorage {
        blobs: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    }

    impl InMemoryBlobStorage {
        fn new() -> Self {
            Self {
                blobs: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl BlobStorage for InMemoryBlobStorage {
        async fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SearchError> {
            let blobs = self.blobs.read().unwrap();
            Ok(blobs.get(name).cloned())
        }

        async fn save(&self, name: &str, data: &[u8]) -> Result<(), SearchError> {
            let mut blobs = self.blobs.write().unwrap();
            blobs.insert(name.to_string(), data.to_vec());
            Ok(())
        }

        async fn delete(&self, name: &str) -> Result<bool, SearchError> {
            let mut blobs = self.blobs.write().unwrap();
            Ok(blobs.remove(name).is_some())
        }

        async fn clear_all(&self) -> Result<(), SearchError> {
            let mut blobs = self.blobs.write().unwrap();
            blobs.clear();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_foundation_engine_index_and_search_body() {
        let storage = InMemoryBlobStorage::new();
        let mut engine = FoundationSearchEngine::new(storage.clone());

        // Index body text (with default metadata for this test)
        let default_metadata = crate::traits::MessageMetadata::default();
        engine
            .index_body(
                "msg-1",
                "Let's discuss the project timeline tomorrow at 10am.",
                &default_metadata,
            )
            .await
            .expect("Indexing should succeed");

        // Verify blobs were saved (index data persisted)
        let blob_count = storage.blobs.read().unwrap().len();
        assert!(blob_count > 0, "Blobs should be saved after indexing");

        // Search should find the indexed document
        let results = engine
            .search_with_metadata("project")
            .await
            .expect("Search should succeed");

        assert_eq!(results.len(), 1, "Should find 1 matching document");
    }

    /// Mock MessageDataProvider for testing
    struct MockMessageDataProvider {
        messages: Arc<
            RwLock<
                HashMap<
                    LocalMessageId,
                    (
                        proton_mail_api::services::proton::common::MessageId,
                        String,
                        MessageMetadata,
                    ),
                >,
            >,
        >,
    }

    impl MockMessageDataProvider {
        fn new() -> Self {
            Self {
                messages: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        fn add_message(
            &self,
            local_id: LocalMessageId,
            remote_id: proton_mail_api::services::proton::common::MessageId,
            body: String,
            metadata: MessageMetadata,
        ) {
            self.messages
                .write()
                .unwrap()
                .insert(local_id, (remote_id, body, metadata));
        }
    }

    #[derive(Debug)]
    struct MockError(String);

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for MockError {}

    #[async_trait::async_trait]
    impl MessageDataProvider for MockMessageDataProvider {
        type Error = MockError;

        async fn get_body(
            &self,
            message_id: LocalMessageId,
        ) -> Result<Option<(String, bool)>, Self::Error> {
            let messages = self.messages.read().unwrap();
            Ok(messages
                .get(&message_id)
                .map(|(_, body, _)| (body.clone(), false)))
        }

        async fn get_remote_id(
            &self,
            message_id: LocalMessageId,
        ) -> Result<Option<proton_mail_api::services::proton::common::MessageId>, Self::Error>
        {
            let messages = self.messages.read().unwrap();
            Ok(messages
                .get(&message_id)
                .map(|(remote_id, _, _)| remote_id.clone()))
        }

        async fn has_local_draft_metadata(
            &self,
            _message_id: LocalMessageId,
        ) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn get_metadata(
            &self,
            message_id: LocalMessageId,
        ) -> Result<Option<MessageMetadata>, Self::Error> {
            let messages = self.messages.read().unwrap();
            Ok(messages
                .get(&message_id)
                .map(|(_, _, metadata)| metadata.clone()))
        }
    }

    /// Full integration test for batch indexing flow
    ///
    /// This test exercises the complete end-to-end flow:
    /// - Intent system (queueing intents in transaction)
    /// - Worker processing (prepare_message_for_indexing, batch preparation)
    /// - Service layer batch indexing
    /// - Foundation engine batch commit
    /// - spawn_blocking for CPU-bound tokenization
    /// - Channel-based async I/O (mpsc + oneshot channels for blob loads)
    /// - Atomic blob saves in transaction
    /// - Intent cleanup (content hash saved, intents deleted)
    #[tokio::test]
    async fn test_full_indexing_flow_with_intent_system() {
        use proton_mail_api::services::proton::common::MessageId;
        use stash::stash::{Stash, StashConfiguration};

        // 1. Set up Stash with migrations
        // Note: In a real scenario, migrations would be run by mail-search
        // For this test, we need to run migrations manually
        let stash = Stash::new(StashConfiguration::test()).unwrap();
        let migrations = crate::migrations::search_migrations();
        let mut tether = stash.connection().await.unwrap();
        use stash::stash::StashError;
        for migration in migrations {
            tether
                .tx::<_, (), StashError>(async |bond| {
                    migration.migrate(&bond).await?;
                    Ok(())
                })
                .await
                .unwrap();
        }

        // 2. Create MailSearchService
        let search_service = MailSearchService::new(stash.clone());

        // 3. Create mock MessageDataProvider with 5 messages
        let data_provider = Arc::new(MockMessageDataProvider::new());
        let message_ids = vec![1, 2, 3, 4, 5];
        let remote_ids = vec![
            MessageId::from("remote-1"),
            MessageId::from("remote-2"),
            MessageId::from("remote-3"),
            MessageId::from("remote-4"),
            MessageId::from("remote-5"),
        ];
        let bodies = vec![
            "Let's discuss the project timeline tomorrow at 10am.",
            "The quarterly report shows strong growth in Q4.",
            "Meeting scheduled for next week to review the budget.",
            "Please send the presentation slides before the conference.",
            "The team completed the sprint ahead of schedule.",
        ];

        for (i, &local_id) in message_ids.iter().enumerate() {
            data_provider.add_message(
                local_id,
                remote_ids[i].clone(),
                bodies[i].to_string(),
                MessageMetadata::default(),
            );
        }

        // 4. Queue intents in a transaction (simulating MessageBody::store)
        let mut tether = stash.connection().await.unwrap();
        use stash::stash::StashError as SE;
        tether
            .tx::<_, (), SE>(async |bond| {
                for &local_id in &message_ids {
                    MailSearchService::queue_index(local_id, &bond).await?;
                }
                Ok(())
            })
            .await
            .unwrap();

        // 5. Verify intents were created
        let tether = stash.connection().await.unwrap();
        let intents = SearchIndexIntent::pop_batch(&tether, 10).await.unwrap();
        assert_eq!(intents.len(), 5, "Should have 5 intents queued");

        // 6. Create worker and process batch through the full worker flow
        // This tests: intent system → worker → prepare_message_for_indexing →
        // batch indexing → spawn_blocking → channel-based I/O → atomic blob saves
        let watcher_handle = crate::watcher::SearchIndexIntentWatcher::watch(&stash)
            .await
            .unwrap();
        let worker = SearchIndexWorker::new(
            stash.clone(),
            search_service.clone(),
            data_provider,
            watcher_handle,
        );

        // Process one batch using the worker
        // This exercises the full worker flow including prepare_message_for_indexing
        let processed = worker.process_batch().await.unwrap();
        assert!(processed, "Worker should have processed the batch");

        // 7. Verify intents were deleted (worker deletes them after successful indexing)
        let tether = stash.connection().await.unwrap();
        let remaining_intents = SearchIndexIntent::pop_batch(&tether, 10).await.unwrap();
        assert_eq!(
            remaining_intents.len(),
            0,
            "All intents should be deleted after successful processing by worker"
        );

        // 9. Verify messages are searchable
        let results = search_service
            .search_local_with_metadata("project")
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "Should find message with 'project'");

        let results = search_service
            .search_local_with_metadata("report")
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "Should find message with 'report'");

        let results = search_service
            .search_local_with_metadata("meeting")
            .await
            .unwrap();
        // "Meeting" appears in msg-3, "meeting" might be case-sensitive
        assert!(
            results.len() >= 1,
            "Should find at least 1 message with 'meeting'"
        );

        // 8. Verify stats are accessible (just checking the method works)
        let _stats = search_service.get_stats().await;

        // Verify search results prove indexing worked (more reliable than stats)
        let all_results = search_service
            .search_local_with_metadata("the")
            .await
            .unwrap();
        assert!(
            all_results.len() >= 5,
            "Should find all 5 messages with common word 'the'"
        );
    }

    #[tokio::test]
    async fn test_foundation_engine_stats() {
        let storage = InMemoryBlobStorage::new();
        let engine = FoundationSearchEngine::new(storage);

        let stats = engine.stats();
        assert_eq!(stats.documents_total, 0);
        assert!(!stats.is_writing);
    }

    #[tokio::test]
    async fn test_foundation_engine_cleanup_empty() {
        let storage = InMemoryBlobStorage::new();
        let mut engine = FoundationSearchEngine::new(storage);

        let result = engine.cleanup().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().blobs_deleted, 0);
    }

    #[tokio::test]
    async fn test_foundation_engine_remove_nonexistent() {
        let storage = InMemoryBlobStorage::new();
        let mut engine = FoundationSearchEngine::new(storage);

        // Removing a message that was never indexed should succeed
        let result = engine.remove_message("nonexistent-msg").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_index_result() {
        let needs = IndexResult::needs_cleanup();
        assert!(needs.cleanup_needed);

        let no_cleanup = IndexResult::no_cleanup();
        assert!(!no_cleanup.cleanup_needed);
    }

    #[test]
    fn test_search_error_display() {
        let err = SearchError::EngineBusy;
        assert_eq!(err.to_string(), "Search engine is busy");

        let err = SearchError::InvalidQuery("bad query".to_string());
        assert_eq!(err.to_string(), "Invalid query: bad query");

        let err = SearchError::Panic("test panic".to_string());
        assert_eq!(err.to_string(), "Engine panic: test panic");
    }
}
