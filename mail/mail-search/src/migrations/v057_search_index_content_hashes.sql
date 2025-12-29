-- Foundation Search: Separate content hash tracking table
--
-- This table persists content hashes independently of intents, allowing us to
-- detect duplicate content even after intents are deleted. This prevents
-- unnecessary re-indexing when content hasn't changed.
--
-- The content_hash is computed from message body + metadata and uniquely
-- identifies the indexed content. If a new intent is created for a message
-- with the same content_hash, we can skip indexing.

CREATE TABLE IF NOT EXISTS search_index_content_hashes (
    message_id INTEGER PRIMARY KEY NOT NULL,    -- LocalMessageId (u64)
    content_hash TEXT NOT NULL,                 -- SHA256 hash of body + metadata
    updated_at INTEGER NOT NULL                  -- Unix timestamp when hash was last updated
) WITHOUT ROWID;