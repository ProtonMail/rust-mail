//! Stash-based blob storage for Foundation Search
//!
//! This module provides a `BlobStorage` implementation that uses Stash
//! (`SQLite`) for persisting search index blobs.

use async_trait::async_trait;
use stash::UserDb;
use stash::stash::Stash;
use tracing::debug;

use crate::error::SearchError;
use crate::traits::BlobStorage;

/// Stash-based implementation of `BlobStorage`
///
/// Stores search index blobs in `SQLite` via the Stash connection pool.
#[derive(Clone)]
pub struct StashBlobStorage {
    stash: Stash<UserDb>,
}

impl StashBlobStorage {
    /// Create a new Stash-based blob storage
    #[must_use]
    pub fn new(stash: Stash<UserDb>) -> Self {
        Self { stash }
    }

    /// Get a reference to the underlying Stash instance
    #[must_use]
    pub fn stash(&self) -> &Stash<UserDb> {
        &self.stash
    }

    /// Save multiple blobs atomically in a single transaction
    ///
    /// This ensures that either all blobs are saved or none are, preventing
    /// orphaned blobs if the operation fails mid-way.
    pub async fn save_batch_atomic(
        &self,
        blobs: Vec<(String, Vec<u8>)>,
    ) -> Result<(), SearchError> {
        use stash::params;
        use stash::stash::StashError as SE;

        if blobs.is_empty() {
            return Ok(());
        }

        let mut tether = self
            .stash
            .connection()
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Failed to get connection: {e}")))?;

        let timestamp = chrono::Utc::now().timestamp();
        let count = blobs.len();
        tether
            .tx::<_, (), SE>(async |bond| {
                for (name, data) in blobs {
                    bond.execute(
                        "INSERT OR REPLACE INTO search_index_blobs (blob_name, blob_data, updated_at)
                         VALUES (?1, ?2, ?3)",
                        params![name, data, timestamp],
                    )
                    .await?;
                }
                Ok(())
            })
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Transaction failed: {e}")))?;

        debug!("Saved {} blobs atomically", count);
        Ok(())
    }
}

#[async_trait]
impl BlobStorage for StashBlobStorage {
    async fn load(&self, name: &str) -> Result<Option<Vec<u8>>, SearchError> {
        let name_owned = name.to_owned();
        let tether = self
            .stash
            .connection()
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Failed to get connection: {e}")))?;

        let blob = tether
            .sync_query(move |conn| {
                use stash::rusqlite::OptionalExtension;
                conn.query_row(
                    "SELECT blob_data FROM search_index_blobs WHERE blob_name = ?1",
                    [&name_owned],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(|e| {
                    stash::stash::StashError::Custom(anyhow::anyhow!(
                        "Failed to load blob '{}': {}",
                        name_owned,
                        e
                    ))
                })
            })
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Query failed: {e}")))?;

        debug!(
            "Loaded blob '{}' ({} bytes)",
            name,
            blob.as_ref().map_or(0, Vec::len)
        );
        Ok(blob)
    }

    async fn save(&self, name: &str, data: &[u8]) -> Result<(), SearchError> {
        use stash::params;
        use stash::stash::StashError as SE;

        let data_len = data.len();
        let name_owned = name.to_owned();
        let data_owned = data.to_vec();
        let timestamp = chrono::Utc::now().timestamp();

        let mut tether = self
            .stash
            .connection()
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Failed to get connection: {e}")))?;

        tether
            .tx::<_, (), SE>(async |bond| {
                bond.execute(
                    "INSERT OR REPLACE INTO search_index_blobs (blob_name, blob_data, updated_at)
                     VALUES (?1, ?2, ?3)",
                    params![name_owned, data_owned, timestamp],
                )
                .await?;
                Ok(())
            })
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Transaction failed: {e}")))?;

        debug!("Saved blob '{}' ({} bytes)", name, data_len);
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<bool, SearchError> {
        use stash::params;
        use stash::stash::StashError as SE;

        let name_owned = name.to_owned();

        let mut tether = self
            .stash
            .connection()
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Failed to get connection: {e}")))?;

        let deleted = tether
            .tx::<_, bool, SE>(async |bond| {
                let rows = bond
                    .execute(
                        "DELETE FROM search_index_blobs WHERE blob_name = ?1",
                        params![name_owned],
                    )
                    .await?;
                Ok(rows > 0)
            })
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Transaction failed: {e}")))?;

        if deleted {
            debug!("Deleted blob '{}'", name);
        }

        Ok(deleted)
    }

    async fn clear_all(&self) -> Result<(), SearchError> {
        use stash::stash::StashError as SE;

        let mut tether = self
            .stash
            .connection()
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Failed to get connection: {e}")))?;

        let deleted_count = tether
            .tx::<_, usize, SE>(async |bond| {
                let count = bond
                    .execute("DELETE FROM search_index_blobs", vec![])
                    .await?;
                Ok(count)
            })
            .await
            .map_err(|e| SearchError::BlobStorage(format!("Transaction failed: {e}")))?;

        debug!("Cleared all {} blobs from storage", deleted_count);
        Ok(())
    }

    async fn save_batch_atomic(&self, blobs: Vec<(String, Vec<u8>)>) -> Result<(), SearchError> {
        // Use the existing implementation that provides transactional guarantees
        StashBlobStorage::save_batch_atomic(self, blobs).await
    }
}
