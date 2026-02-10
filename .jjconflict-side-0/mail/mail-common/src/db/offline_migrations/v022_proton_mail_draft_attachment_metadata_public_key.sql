ALTER TABLE draft_attachment_metadata
    ADD COLUMN is_public_key INTEGER NOT NULL DEFAULT 0;
