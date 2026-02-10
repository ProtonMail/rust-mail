ALTER TABLE mail_conversation_scroll_data
    ADD COLUMN snooze_time INTEGER DEFAULT 0;

ALTER TABLE mail_message_scroll_data
    ADD COLUMN snooze_time INTEGER DEFAULT 0;

--- Clear Inbox and Snoozed labels ---
DELETE FROM mail_conversation_scroll_data
WHERE local_label_id IN (1, 14);

DELETE FROM mail_message_scroll_data
WHERE local_label_id IN (1, 14);