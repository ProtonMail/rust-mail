//! Error types for search operations

use thiserror::Error;

/// Errors that can occur during search operations
#[derive(Error, Debug)]
pub enum SearchError {
    /// Search engine is currently busy (another write in progress)
    ///
    /// **Note**: This error is unreachable in production code because writes are serialized
    /// by the `RwLock` in `MailSearchService`. If this error occurs, it indicates a bug
    /// in the serialization logic. Kept for test compatibility.
    #[error("Search engine is busy")]
    EngineBusy,

    /// Failed to parse the search query
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Failed to load or save blob data
    #[error("Blob storage error: {0}")]
    BlobStorage(String),

    /// Document not found in index
    #[error("Document not found: {0}")]
    NotFound(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Internal engine error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Engine panicked during operation
    #[error("Engine panic: {0}")]
    Panic(String),
}

impl From<anyhow::Error> for SearchError {
    fn from(err: anyhow::Error) -> Self {
        SearchError::Internal(err.to_string())
    }
}
