-- Foundation Search: Index blob storage
--
-- This table stores the Foundation Search engine's index blobs.
-- Each blob represents a serialized portion of the search index.
--
-- The Foundation Search engine uses a "sans-IO" design where index data
-- is persisted through Load/Save events. This table acts as the storage
-- backend for those events.

CREATE TABLE IF NOT EXISTS search_index_blobs (
    blob_name TEXT PRIMARY KEY NOT NULL,      -- Unique blob identifier (e.g., "manifest", "text_0", "text_1")
    blob_data BLOB NOT NULL,                   -- Serialized index data (CBOR format)
    updated_at INTEGER NOT NULL                -- Unix timestamp of last update
) WITHOUT ROWID;

-- Index for timestamp-based queries (useful for cleanup/debugging)
CREATE INDEX IF NOT EXISTS idx_search_index_blobs_updated 
    ON search_index_blobs(updated_at);
