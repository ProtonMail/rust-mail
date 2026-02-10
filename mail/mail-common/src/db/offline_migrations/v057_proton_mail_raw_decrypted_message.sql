CREATE TABLE raw_message_body
(
    message_id       INTEGER PRIMARY KEY,
    raw_type         INTEGER NOT NULL,
    body             BLOB    NOT NULL,
    signatures       BLOB    NOT NULL,
    decryption_error TEXT DEFAULT NULL,
    raw_message_id   TEXT DEFAULT NULL,

    CONSTRAINT message_body_id
        FOREIGN KEY (message_id)
            REFERENCES messages (local_id)
            ON DELETE CASCADE
);


-- Copy over all existing draft bodies as plain text data
INSERT INTO raw_message_body
SELECT mb.message_id, 0, mb.body, '', NULL, NULL
FROM message_body as mb
WHERE mb.message_id IN (SELECT local_message_id FROM draft_metadata WHERE local_message_id IS NOT NULL);

-- Delete message bodies;
DROP TABLE message_body;