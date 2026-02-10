-- Restore any broken pgp embedded attachment links that may have been lost.
INSERT OR IGNORE INTO message_attachments (local_message_id, local_attachment_id)
SELECT local_message_id, local_id
FROM attachments
WHERE attachment_type = '"Pgp"';