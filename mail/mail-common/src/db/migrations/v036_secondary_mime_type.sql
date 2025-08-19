ALTER TABLE message_body
ADD COLUMN mime_type STRING;

UPDATE message_body
SET mime_type = 'text/html'
WHERE message_id IN (
    SELECT mb.local_message_id
    FROM message_bodies mb
    WHERE mb.mime_type = 6
);

UPDATE message_body
SET mime_type = 'text/plain'
WHERE message_id IN (
    SELECT mb.local_message_id
    FROM message_bodies mb
    WHERE mb.mime_type = 7
);

-- Now the only remaining messages are those with mime-encryption - since we
-- cannot figure out their "decrypted" mime types post factum, let's remove
-- those messages, forcing Rust code to re-decrypt them, filling out the correct
-- type the next time the message is accessed.
DELETE FROM message_body WHERE mime_type IS NULL;
DELETE FROM message_bodies WHERE mime_type != 6 AND mime_type != 7;
