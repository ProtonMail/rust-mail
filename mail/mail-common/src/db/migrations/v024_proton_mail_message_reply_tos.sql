CREATE TABLE message_reply_to
(
    local_message_id     INTEGER PRIMARY KEY,
    name                 TEXT    NOT NULL,
    address              TEXT    NOT NULL,
    bimi_selector        TEXT             DEFAULT NULL,
    is_proton            INTEGER NOT NULL DEFAULT 0,
    is_simple_login      INTEGER NOT NULL DEFAULT 0,
    display_sender_image INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT message_reply_to_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

CREATE TABLE message_reply_tos
(
    local_message_id     INTEGER PRIMARY KEY,
    name                 TEXT    NOT NULL,
    address              TEXT    NOT NULL,
    bimi_selector        TEXT             DEFAULT NULL,
    is_proton            INTEGER NOT NULL DEFAULT 0,
    is_simple_login      INTEGER NOT NULL DEFAULT 0,
    display_sender_image INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT message_reply_to_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);