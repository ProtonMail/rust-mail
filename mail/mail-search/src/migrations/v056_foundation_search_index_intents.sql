-- Foundation Search: Index Intent Tables
--
-- These tables implement the "Intent List" pattern for search indexing.
-- Instead of spawning background tasks directly, we persist indexing
-- intents to the database and process them via a dedicated worker.
--
-- Benefits:
-- - Serialization: Single worker processes intents one at a time
-- - Retry: Failed intents stay in table with retry_count
-- - Crash Safety: Intents persist across app restarts
-- - No Silent Drops: Work is never lost
-- - Transaction Atomicity: Intent created in same transaction as body store

-- Table for message index/remove intents
-- Uses composite primary key (message_id, operation) - no artificial id needed
CREATE TABLE IF NOT EXISTS search_index_intents (
    message_id INTEGER NOT NULL,                -- LocalMessageId (u64)
    operation TEXT NOT NULL,                    -- 'index' | 'remove'
    retry_count INTEGER NOT NULL DEFAULT 0,     -- Number of failed attempts
    created_at INTEGER NOT NULL,                -- Unix timestamp when intent was created
    PRIMARY KEY (message_id, operation)         -- Composite PK: one intent per message+operation
) WITHOUT ROWID;                                -- Optimization for composite PK tables

-- Index for efficient worker polling (oldest first)
CREATE INDEX IF NOT EXISTS idx_search_index_intents_created 
    ON search_index_intents(created_at ASC);

-- Note: No separate cleanup table needed.
-- The worker calls cleanup() whenever the intent queue is empty.
-- Foundation Search's cleanup is idempotent (returns 0 if nothing to clean).
