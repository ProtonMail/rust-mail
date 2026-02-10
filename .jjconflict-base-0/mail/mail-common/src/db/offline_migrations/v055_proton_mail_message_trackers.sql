CREATE TABLE message_trackers (
    local_message_id INTEGER PRIMARY KEY NOT NULL,
    last_checked_at INTEGER NOT NULL, -- Unix timestamp
    CONSTRAINT tracked_messages_mid
        FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);
