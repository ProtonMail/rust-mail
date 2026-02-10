-- This migration will select all messages and set their snooze_time to the maximum context_snooze_time from conversation_labels for their conversation
-- Additionally it will look up conversation.display_snooze_reminder and if it is 1 it will set display_snooze_reminder of all messages to 1 which are within this conversation

-- Update snooze_time for all messages by taking the maximum context_snooze_time from conversation_labels
UPDATE messages
SET snooze_time = (
    SELECT MAX(cl.context_snooze_time)
    FROM conversation_labels cl
    WHERE cl.local_conversation_id = messages.local_conversation_id
    AND cl.context_snooze_time > 0
)
WHERE EXISTS (
    SELECT 1
    FROM conversation_labels cl
    WHERE cl.local_conversation_id = messages.local_conversation_id
    AND cl.context_snooze_time > 0
);

-- Update display_snooze_reminder for all messages in conversations where display_snooze_reminder = 1
UPDATE messages
SET display_snooze_reminder = 1
WHERE local_conversation_id IN (
    SELECT local_id
    FROM conversations
    WHERE display_snooze_reminder = 1
);

-- Clear message scroller cursors to force refresh order after snooze_time update
DELETE FROM mail_message_scroll_data;