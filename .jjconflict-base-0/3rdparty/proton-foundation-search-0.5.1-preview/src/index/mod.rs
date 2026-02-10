//! Core indexing functionality for the search engine.
//!
//! This module provides the infrastructure for maintaining and querying search indexes.
//! It handles partitioning of data, caching, and coordination between different index types.
//!
//! # Architecture
//!
//! The indexing system is built around these key components:
//!
//! - `Manager`: Coordinates access to index partitions and handles partition splitting
//! - `PartitionFileCache`: Caches index files in memory for better performance
//! - `Partition`: Contains the actual index data for a subset of documents
//! - `FileManifest`: Tracks metadata about index files on disk
//!
//! # Partitioning
//!
//! The index automatically splits into partitions when they grow beyond a configured size limit.
//! This helps maintain consistent performance with large datasets by:
//!
//! - Keeping individual index files at a manageable size
//! - Allowing parallel search across partitions
//! - Supporting incremental updates without re-indexing everything
//!
//! # Caching
//!
//! The `PartitionFileCache` maintains a LRU cache of recently accessed index files to:
//!
//! - Reduce disk I/O for frequently accessed data
//! - Batch writes for better performance
//! - Support recovery after crashes
//!
//! The cache size is configurable via `ManagerConfig`.
//!
//! # Thread Safety
//!
//! The indexing system is designed to be thread-safe and support concurrent access:
//!
//! - Multiple readers can access indexes simultaneously
//! - Writers get exclusive access during updates
//! - The cache handles concurrent access to shared data
//!
//! # Error Handling
//!
//! Most operations return `Result<T, std::io::Error>` to handle:
//!
//! - File system errors
//! - Index corruption
//! - Invalid data formats
//!
//! # Performance
//!
//! The indexing system optimizes for:
//!
//! - Fast queries via partitioning and caching
//! - Efficient updates through batching
//! - Minimal memory usage with configurable cache sizes
//! - Concurrent access where possible

pub(crate) mod collection;
pub(crate) mod extensions;
pub mod prelude;
/// Text indexing and search functionality
pub mod text;
pub mod trivial;
/// WAL-based index management system
pub mod wal;
