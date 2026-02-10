CREATE TABLE message_attachments_metadata
(
    local_message_id    INTEGER NOT NULL,
    local_attachment_id INTEGER NOT NULL,
    PRIMARY KEY (local_message_id, local_attachment_id),
    CONSTRAINT message_attachments_cid FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT message_attachments_aid FOREIGN KEY (local_attachment_id) REFERENCES attachments (local_id) ON DELETE CASCADE ON UPDATE CASCADE
);


-- Copy existing information
INSERT INTO message_attachments_metadata
SELECT *
FROM message_attachments;

-- Delete all message body metadata that are not drafts to reset state.
DELETE
FROM message_bodies
WHERE local_message_id NOT IN (SELECT local_message_id FROM draft_metadata WHERE local_message_id IS NOT NULL);

DELETE
FROM message_body
WHERE message_id NOT IN (SELECT local_message_id FROM draft_metadata WHERE local_message_id IS NOT NULL);

