-- It is safe to add `IF NOT EXISTS` here since it will not do anything if the table already exists
-- And for fresh installs it will be compiting with the core migration which will create the table
-- to ensure that there are no migration errors when running both migrations together it has to be
-- either modified or removed in oppose to adding new mail migration step which will be run after
-- core migration anyway.
CREATE TABLE IF NOT EXISTS event_id_store (id TEXT PRIMARY KEY, value TEXT NOT NULL)
