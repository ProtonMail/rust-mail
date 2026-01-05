CREATE TABLE message_tracker_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_message_id INTEGER NOT NULL,
    tracker_domain TEXT NOT NULL,
    original_url TEXT NOT NULL,
    FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);
