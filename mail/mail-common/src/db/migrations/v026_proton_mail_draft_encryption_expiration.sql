ALTER TABLE draft_metadata
    ADD COLUMN
        expiration_time INTEGER DEFAULT NULL;

ALTER TABLE draft_metadata
    ADD COLUMN
        password BLOB DEFAULT NULL;

ALTER TABLE draft_metadata
    ADD COLUMN
        password_hint TEXT DEFAULT NULL;
