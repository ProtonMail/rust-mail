CREATE TABLE message_reply_to
(
    local_message_id INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    address          TEXT NOT NULL,
    bimi_selector    TEXT DEFAULT NULL,
    is_proton        INTEGER NOT NULL DEFAULT 0,
    is_simple_login  INTEGER NOT NULL DEFAULT 0,
    display_sender_image INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT message_reply_to_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

CREATE TABLE message_reply_tos
(
    local_message_id INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    address          TEXT NOT NULL,
    bimi_selector    TEXT DEFAULT NULL,
    is_proton        INTEGER NOT NULL DEFAULT 0,
    is_simple_login  INTEGER NOT NULL DEFAULT 0,
    display_sender_image INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT message_reply_to_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

-- DELETE all messages that are not active drafts
DELETE
FROM messages
WHERE local_id NOT IN (SELECT local_message_id
                       FROM draft_metadata);

ALTER TABLE messages DROP COLUMN reply_tos;


-- DELETE all cached message scroller data
DELETE FROM mail_message_scroll_data;

-- We don't have to delete all conversations but we need to reset the fact that they don't have messages
UPDATE conversations SET has_messages =0;
