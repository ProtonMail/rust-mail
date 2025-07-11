--- Ensure the that we only have one session per user id, we grab the latest entry for a user and delete
--- all other entries to avoid duplicates.
WITH max_ids AS (
    SELECT MAX(rowid), remote_id FROM core_sessions GROUP BY account_id
)
DELETE FROM core_sessions WHERE remote_id NOT IN (
    SELECT remote_id FROM max_ids
);

--- Create a unique index for the user id.
CREATE UNIQUE INDEX index_core_sessions_account_id ON core_sessions (account_id);
