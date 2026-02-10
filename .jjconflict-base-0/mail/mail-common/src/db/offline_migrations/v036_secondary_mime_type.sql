-- Remove mime-encrypted messages - we cannot figure out message_body.mime_type
-- for those, they need to get re-encrypted again.
DELETE FROM message_bodies WHERE mime_type != 6 AND mime_type != 7;

CREATE TABLE message_body_v2 (
    message_id INTEGER PRIMARY KEY,
    body TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    decryption_error TEXT DEFAULT NULL,

    CONSTRAINT message_body_id
        FOREIGN KEY (message_id)
        REFERENCES messages (local_id)
        ON DELETE CASCADE
);

INSERT INTO message_body_v2 (message_id, body, mime_type, decryption_error)
SELECT message_id, body, 'text/html', decryption_error
FROM message_body
WHERE message_id IN (
    SELECT mb.local_message_id
    FROM message_bodies mb
    WHERE mb.mime_type = 6
);

INSERT INTO message_body_v2 (message_id, body, mime_type, decryption_error)
SELECT message_id, body, 'text/plain', decryption_error
FROM message_body
WHERE message_id IN (
    SELECT mb.local_message_id
    FROM message_bodies mb
    WHERE mb.mime_type = 7
);

DROP TABLE message_body;
ALTER TABLE message_body_v2 RENAME TO message_body;
