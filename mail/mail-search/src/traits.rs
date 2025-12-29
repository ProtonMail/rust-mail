//! External traits that must be implemented by the consuming crate (mail-common)
//!
//! These traits define the boundary between mail-search and mail-common.
//! mail-search is self-contained with its own DB schema, but needs to
//! fetch message data (body, remote ID) from the main message store.

use async_trait::async_trait;
use proton_mail_api::services::proton::common::MessageId;

use crate::intent::LocalMessageId;

#[cfg(feature = "foundation_search")]
use crate::error::SearchError;

/// Provides message data for search indexing
///
/// This trait is implemented by mail-common to provide access to message
/// bodies and remote IDs without mail-search needing to know about the
/// Message or MessageBody models.
#[async_trait]
pub trait MessageDataProvider: Send + Sync {
    /// Error type for data provider operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Get the decrypted message body for indexing
    ///
    /// Returns `None` if:
    /// - Message doesn't exist
    /// - Body hasn't been stored yet
    /// - Decryption failed
    ///
    /// Returns `Some((body, is_html))` where:
    /// - `body` is the decrypted message body content
    /// - `is_html` is `true` if the MIME type is `text/html`, `false` for `text/plain`
    async fn get_body(
        &self,
        message_id: LocalMessageId,
    ) -> Result<Option<(String, bool)>, Self::Error>;

    /// Get the remote (server) message ID
    ///
    /// Returns `None` if:
    /// - Message doesn't exist
    /// - Message hasn't been synced to server yet (e.g., local draft)
    async fn get_remote_id(
        &self,
        message_id: LocalMessageId,
    ) -> Result<Option<MessageId>, Self::Error>;

    /// Check if a message has local draft metadata (is being edited locally)
    ///
    /// Messages being edited locally are skipped during indexing because their
    /// content is incomplete. They will be indexed when sent.
    ///
    /// Note: This only skips drafts with local `DraftMetadata`. Drafts that
    /// exist but aren't being edited locally (e.g., synced from another device)
    /// will still be indexed.
    async fn has_local_draft_metadata(
        &self,
        message_id: LocalMessageId,
    ) -> Result<bool, Self::Error>;

    /// Get message metadata for indexing (subject, sender, recipients)
    ///
    /// Returns `None` if:
    /// - Message doesn't exist
    ///
    /// Returns `Some(metadata)` with subject and email addresses for search.
    async fn get_metadata(
        &self,
        message_id: LocalMessageId,
    ) -> Result<Option<MessageMetadata>, Self::Error>;
}

/// Message metadata for search indexing
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct MessageMetadata {
    /// Message subject
    pub subject: String,
    /// Sender email address
    pub from: String,
    /// To recipients (comma-separated email addresses)
    pub to: String,
    /// CC recipients (comma-separated email addresses)
    pub cc: String,
    /// BCC recipients (comma-separated email addresses)
    pub bcc: String,
}

impl MessageMetadata {
    /// Compute a content hash for duplicate detection
    ///
    /// This hash represents the searchable content of a message (body + metadata).
    /// If the hash matches a previously indexed message, we can skip re-indexing.
    pub fn compute_content_hash(body: &str, metadata: Option<&Self>) -> String {
        use sha2::{Digest, Sha256};
        use std::hash::{Hash, Hasher};

        let mut sha256 = Sha256::new();
        sha256.update(body.as_bytes());

        if let Some(meta) = metadata {
            let mut hasher = std::hash::DefaultHasher::new();
            meta.hash(&mut hasher);
            let hash_value = hasher.finish();
            sha256.update(hash_value.to_le_bytes());
        }

        hex::encode(sha256.finalize())
    }
}

/// Trait for blob storage backends
///
/// This trait exists to decouple `mail-search` from the `stash` crate,
/// allowing the search engine to remain storage-agnostic. While there's
/// currently only one implementation (`StashBlobStorage` in `mail-search/src/storage.rs`),
/// the trait enables unit testing with mock storage if needed.
///
/// Implementations must provide async load/save operations for index blobs.
#[cfg(feature = "foundation_search")]
#[async_trait]
pub trait BlobStorage: Send + Sync {
    /// Load a blob by name, returning None if not found
    async fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SearchError>;

    /// Save a blob with the given name
    async fn save(&self, name: &str, data: &[u8]) -> Result<(), SearchError>;

    /// Delete a blob by name
    async fn delete(&self, name: &str) -> Result<bool, SearchError>;

    /// Clear all blobs from storage
    ///
    /// Removes all stored blobs, effectively clearing the entire index.
    /// Used by the clear() method to reset the search index.
    async fn clear_all(&self) -> Result<(), SearchError>;
}
