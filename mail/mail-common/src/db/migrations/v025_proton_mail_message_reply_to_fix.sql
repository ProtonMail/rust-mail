-- Run this again to correct issues from the previous migration that has an incorrect query.
DELETE
FROM message_bodies
WHERE local_message_id NOT IN (SELECT local_message_id FROM draft_metadata WHERE local_message_id IS NOT NULL);

DELETE
FROM message_body
WHERE message_id NOT IN (SELECT local_message_id FROM draft_metadata WHERE local_message_id IS NOT NULL);
