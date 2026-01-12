//! Search Index Intent Model
//!
//! Implements the Transactional Outbox pattern (Chris Richardson)
//! for search indexing. Intents are persisted atomically with message storage
//! and processed asynchronously by a background worker, ensuring crash safety
//! and reliable retry semantics.

use std::fmt;

use stash::params;
use stash::rusqlite::OptionalExtension;
use stash::rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use stash::stash::{Bond, StashError, Tether};
use tracing::{debug, warn};

/// Local message ID type (u64 wrapped for type safety)
///
/// This is a copy of `LocalMessageId` from mail-common to avoid circular dependencies.
pub type LocalMessageId = u64;

/// The operation to perform on the search index
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchOperation {
    /// Index the message body
    Index,
    /// Remove the message from the index
    Remove,
}

impl SearchOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Remove => "remove",
        }
    }
}

impl fmt::Display for SearchOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ToSql for SearchOperation {
    fn to_sql(&self) -> stash::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(
            self.as_str().as_bytes(),
        )))
    }
}

impl FromSql for SearchOperation {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "index" => Ok(Self::Index),
            "remove" => Ok(Self::Remove),
            other => Err(FromSqlError::Other(
                format!("Invalid SearchOperation: {other}").into(),
            )),
        }
    }
}

/// A search index intent - a persistent record of work to be done
///
/// Uses composite primary key (`message_id`, operation) instead of artificial id.
/// Content hashes are stored in a separate `search_index_content_hashes` table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchIndexIntent {
    pub message_id: LocalMessageId,
    pub operation: SearchOperation,
    pub retry_count: u64,
    pub created_at: i64,
}

impl SearchIndexIntent {
    /// Create or update an intent (upsert)
    ///
    /// Uses INSERT OR IGNORE to handle the PRIMARY KEY constraint on (`message_id`, operation).
    /// If an intent already exists for this message+operation, no new row is created.
    ///
    /// If `content_hash` is provided, it will be stored. The `content_hash` check for duplicate
    /// detection happens in the worker when processing the intent, not here.
    ///
    /// Returns `true` if a new intent was created or updated, `false` if an intent already
    /// existed and was unchanged.
    pub async fn create_or_ignore(
        message_id: LocalMessageId,
        operation: SearchOperation,
        bond: &Bond<'_>,
    ) -> Result<bool, StashError> {
        // Note: Content hash check happens in the worker, not here, because
        // we can't query within a Bond transaction. The worker will check
        // the separate content_hashes table and skip if hash matches.
        let timestamp = chrono::Utc::now().timestamp();

        // Single INSERT OR IGNORE statement
        // Note: This uses IGNORE, so if intent exists, it won't create a duplicate.
        let rows_affected = bond
            .execute(
                "INSERT OR IGNORE INTO search_index_intents (message_id, operation, retry_count, created_at) 
                 VALUES (?1, ?2, 0, ?3)",
                params![message_id, operation, timestamp],
            )
            .await?;

        let created = rows_affected > 0;
        if created {
            debug!(
                "Created search intent: {} for message {}",
                operation, message_id
            );
        } else {
            debug!(
                "Intent already exists: {} for message {} (skipped)",
                operation, message_id
            );
        }
        Ok(created)
    }

    /// Create or update multiple intents in a single batch operation
    ///
    /// Uses INSERT OR IGNORE to handle the PRIMARY KEY constraint on (`message_id`, operation).
    /// If an intent already exists for a message+operation, no new row is created.
    ///
    /// This is more efficient than calling `create_or_ignore` multiple times
    /// because it performs a single SQL statement for all intents.
    pub async fn create_or_ignore_batch(
        message_ids: &[LocalMessageId],
        operation: SearchOperation,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        // Build a batch INSERT statement with multiple VALUES clauses
        // SQLite supports up to 999 parameters, so we need to batch if needed
        const BATCH_SIZE: usize = 300; // 300 * 3 params = 900 params (safe limit)

        if message_ids.is_empty() {
            return Ok(());
        }

        let timestamp = chrono::Utc::now().timestamp();

        for chunk in message_ids.chunks(BATCH_SIZE) {
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|i| format!("(?{}, ?{}, ?{})", i * 3 + 1, i * 3 + 2, i * 3 + 3))
                .collect();

            let query = format!(
                "INSERT OR IGNORE INTO search_index_intents (message_id, operation, retry_count, created_at) 
                 VALUES {}",
                placeholders.join(", ")
            );

            // Build parameters: for each message_id, add (message_id, operation, timestamp)
            // Need to box values to satisfy ToSql + Send trait bound
            let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
            for message_id in chunk {
                params.push(Box::new(*message_id));
                params.push(Box::new(operation));
                params.push(Box::new(timestamp));
            }

            bond.execute(&query, params).await?;
        }

        debug!(
            "Created {} search intents: {} for {} messages",
            message_ids.len(),
            operation,
            message_ids.len()
        );
        Ok(())
    }

    /// Get the next pending intent to process (oldest first)
    ///
    /// Returns the oldest intent without removing it.
    /// Deletion happens after successful processing via `delete()`.
    pub async fn get_pending(tether: &Tether) -> Result<Option<Self>, StashError> {
        tether
            .sync_query(|conn| {
                conn.query_row(
                    "SELECT message_id, operation, retry_count, created_at 
                     FROM search_index_intents 
                     ORDER BY created_at ASC 
                     LIMIT 1",
                    [],
                    |row| {
                        let retry_count: u64 = row.get(2)?;
                        Ok(Self {
                            message_id: row.get(0)?,
                            operation: row.get(1)?,
                            retry_count,
                            created_at: row.get(3)?,
                        })
                    },
                )
                .optional()
                .map_err(StashError::from)
            })
            .await
    }

    /// Get a batch of pending intents to process (oldest first)
    ///
    /// Returns up to `limit` intents without removing them.
    /// Deletion happens after successful processing via `delete()`.
    pub async fn get_pending_batch(tether: &Tether, limit: usize) -> Result<Vec<Self>, StashError> {
        // Safe cast: usize to i64 for SQLite LIMIT clause
        // In practice, limit will be much smaller than i64::MAX
        #[allow(clippy::cast_possible_wrap)]
        let limit_i64 = limit as i64;
        tether
            .sync_query(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT message_id, operation, retry_count, created_at 
                     FROM search_index_intents 
                     ORDER BY created_at ASC 
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map([limit_i64], |row| {
                    let retry_count: u64 = row.get(2)?;
                    Ok(Self {
                        message_id: row.get(0)?,
                        operation: row.get(1)?,
                        retry_count,
                        created_at: row.get(3)?,
                    })
                })?;
                let mut intents = Vec::new();
                for row in rows {
                    intents.push(row?);
                }
                Ok(intents)
            })
            .await
    }

    /// Mark intent as failed and increment retry count
    ///
    /// The increment is done in SQL with RETURNING to get the accurate count,
    /// avoiding race conditions with concurrent updates.
    pub async fn mark_failed(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        use stash::utils::ConnectionExt;

        // Safe cast: LocalMessageId (u64) to i64 for SQLite
        // In practice, message IDs will be much smaller than i64::MAX
        #[allow(clippy::cast_possible_wrap)]
        let message_id = self.message_id as i64;
        let operation = self.operation.as_str();

        // Increment in SQL and get the actual value back via RETURNING
        let new_count: Option<u64> = bond
            .sync_bridge(move |tx| {
                tx.query_row_col(
                    "UPDATE search_index_intents SET retry_count = retry_count + 1 \
                     WHERE message_id = ?1 AND operation = ?2 \
                     RETURNING retry_count",
                    (message_id, operation),
                )
                .optional()
                .map_err(StashError::from)
            })
            .await?;

        // Update local state to match the database
        if let Some(count) = new_count {
            self.retry_count = count;
        }

        warn!(
            "Search intent failed (retry {}): {} for message {}",
            self.retry_count, self.operation, self.message_id
        );
        Ok(())
    }

    /// Delete this intent (after successful processing)
    pub async fn delete(&self, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM search_index_intents WHERE message_id = ?1 AND operation = ?2",
            params![self.message_id, self.operation],
        )
        .await?;
        Ok(())
    }

    /// Get count of pending intents
    pub async fn pending_count(tether: &Tether) -> Result<i64, StashError> {
        tether
            .sync_query(|conn| {
                conn.query_row("SELECT COUNT(*) FROM search_index_intents", [], |row| {
                    row.get(0)
                })
                .map_err(StashError::from)
            })
            .await
    }

    /// Check if there are any pending intents
    pub async fn has_pending(tether: &Tether) -> Result<bool, StashError> {
        let count = Self::pending_count(tether).await?;
        Ok(count > 0)
    }

    /// Defer this intent by updating its `created_at` timestamp to a future time
    ///
    /// This pushes the intent to the back of the queue, allowing other intents
    /// to be processed first. Useful when an intent can't be processed yet but
    /// will likely be processable later (e.g., message waiting for remote ID).
    pub async fn defer(&self, bond: &Bond<'_>, delay_seconds: i64) -> Result<(), StashError> {
        use stash::params;

        // Safe cast: LocalMessageId (u64) to i64 for SQLite
        // In practice, message IDs will be much smaller than i64::MAX
        #[allow(clippy::cast_possible_wrap)]
        let message_id = self.message_id as i64;
        let operation = self.operation.as_str();
        let new_timestamp = chrono::Utc::now().timestamp() + delay_seconds;

        bond.execute(
            "UPDATE search_index_intents SET created_at = ?1 
             WHERE message_id = ?2 AND operation = ?3",
            params![new_timestamp, message_id, operation],
        )
        .await?;

        debug!(
            "Deferred intent: {} for message {} (new timestamp: {})",
            self.operation, self.message_id, new_timestamp
        );
        Ok(())
    }
}

/// Content hash management functions
///
/// These functions manage the separate `search_index_content_hashes` table
/// which persists content hashes independently of intents. This allows us
/// to detect duplicate content even after intents are deleted.
impl SearchIndexIntent {
    /// Save or update the content hash for a message
    ///
    /// This should be called after successfully indexing a message, before
    /// deleting the intent. The hash persists even after the intent is deleted,
    /// allowing future duplicate detection.
    pub async fn save_content_hash(
        message_id: LocalMessageId,
        content_hash: &str,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let timestamp = chrono::Utc::now().timestamp();
        let hash = content_hash.to_string();
        bond.execute(
            "INSERT OR REPLACE INTO search_index_content_hashes (message_id, content_hash, updated_at) 
             VALUES (?1, ?2, ?3)",
            params![message_id, hash, timestamp],
        )
        .await?;
        Ok(())
    }

    /// Get the content hash for a message, if it exists
    ///
    /// Returns the stored content hash for the given `message_id`, or None
    /// if no hash has been stored yet.
    pub async fn get_content_hash(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<String>, StashError> {
        use stash::rusqlite::OptionalExtension;
        let result = tether
            .sync_query(move |conn| {
                conn.query_row(
                    "SELECT content_hash FROM search_index_content_hashes WHERE message_id = ?1",
                    (message_id,),
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StashError::from)
            })
            .await?;
        Ok(result)
    }

    /// Check if a message's content hash matches the provided hash
    ///
    /// Returns true if the stored hash matches the provided hash, false otherwise.
    /// Returns false if no hash is stored for this message.
    pub async fn content_hash_matches(
        message_id: LocalMessageId,
        content_hash: &str,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        let stored_hash = Self::get_content_hash(message_id, tether).await?;
        Ok(stored_hash.is_some_and(|h| h == content_hash))
    }

    /// Delete the content hash for a message
    ///
    /// This should be called when a message is removed from the index,
    /// to clean up the hash record.
    pub async fn delete_content_hash(
        message_id: LocalMessageId,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM search_index_content_hashes WHERE message_id = ?1",
            params![message_id],
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_display() {
        assert_eq!(format!("{}", SearchOperation::Index), "index");
        assert_eq!(format!("{}", SearchOperation::Remove), "remove");
    }

    #[test]
    fn test_operation_to_sql() {
        // ToSql produces the expected string representation
        let index_sql = SearchOperation::Index.to_sql().unwrap();
        let remove_sql = SearchOperation::Remove.to_sql().unwrap();

        match index_sql {
            ToSqlOutput::Borrowed(ValueRef::Text(bytes)) => {
                assert_eq!(std::str::from_utf8(bytes).unwrap(), "index");
            }
            _ => panic!("Expected Text output"),
        }

        match remove_sql {
            ToSqlOutput::Borrowed(ValueRef::Text(bytes)) => {
                assert_eq!(std::str::from_utf8(bytes).unwrap(), "remove");
            }
            _ => panic!("Expected Text output"),
        }
    }

    #[test]
    fn test_operation_from_sql() {
        // FromSql correctly parses valid strings
        assert_eq!(
            SearchOperation::column_result(ValueRef::Text(b"index")).unwrap(),
            SearchOperation::Index
        );
        assert_eq!(
            SearchOperation::column_result(ValueRef::Text(b"remove")).unwrap(),
            SearchOperation::Remove
        );

        // FromSql rejects invalid strings
        assert!(SearchOperation::column_result(ValueRef::Text(b"unknown")).is_err());
    }
}
