ALTER TABLE conversations
    RENAME COLUMN display_snooze_reminder TO display_snoozed_reminder;

UPDATE conversations AS c
SET display_snoozed_reminder = 1
WHERE EXISTS (
    SELECT 1
    FROM messages AS m
    WHERE m.local_conversation_id = c.local_id
    AND m.snooze_time > 0
);
