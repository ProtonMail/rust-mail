ALTER TABLE draft_metadata
    ADD COLUMN
        expiration_option INTEGER NOT NULL DEFAULT 0;
