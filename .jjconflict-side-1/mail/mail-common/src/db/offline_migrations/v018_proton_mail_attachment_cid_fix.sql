--- Update all stored cid formats to strip the <>
UPDATE attachments
SET content_id = rtrim(ltrim(content_id, '<'), '>')
WHERE content_id IS NOT NULL;