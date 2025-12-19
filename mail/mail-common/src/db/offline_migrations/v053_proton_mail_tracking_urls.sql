CREATE TABLE tracking_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_message_id INTEGER NOT NULL,
    tracker_domain TEXT NOT NULL,
    original_url TEXT NOT NULL,
    CONSTRAINT tracking_urls_mid
        FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);

CREATE INDEX index_tracking_urls_mid ON tracking_urls (local_message_id);
CREATE INDEX index_tracking_urls_domain ON tracking_urls (tracker_domain);
