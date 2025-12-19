CREATE TABLE tracked_messages (
    local_message_id INTEGER PRIMARY KEY NOT NULL,
    status INTEGER NOT NULL, -- 0=Unknown, 1=NoTrackers, 2=Trackers
    last_checked_at INTEGER NOT NULL, -- Unix timestamp
    CONSTRAINT tracked_messages_mid
        FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);

CREATE INDEX index_tracked_messages_status ON tracked_messages (status);
