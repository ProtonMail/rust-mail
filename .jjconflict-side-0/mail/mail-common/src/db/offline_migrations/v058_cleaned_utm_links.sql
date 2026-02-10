CREATE TABLE message_utm_links (
    local_message_id INTEGER PRIMARY KEY NOT NULL,
    CONSTRAINT utm_links_mid
        FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);

CREATE TABLE message_utm_link_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_message_id INTEGER NOT NULL,
    original_url TEXT NOT NULL,
    cleaned_url TEXT NOT NULL,
    FOREIGN KEY (local_message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);

CREATE INDEX index_message_utm_link_urls_local_message_id ON message_utm_link_urls (local_message_id);
