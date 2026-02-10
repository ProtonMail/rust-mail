CREATE TABLE IF NOT EXISTS event_id_store (id TEXT PRIMARY KEY, value TEXT NOT NULL);

-- It is safe to be run in the future when mail event will be different from core event
-- as this migration step will always be run together with the mail event migration
-- which means that both values will be empty and the insert will not do anything.
-- However for now it is essential to run since current users already have the mail event id store
-- and we don't want to lose any events.
INSERT OR IGNORE INTO event_id_store (id, value)
SELECT 'proton-core-event', value
FROM event_id_store
WHERE id = 'proton-mail-event';
