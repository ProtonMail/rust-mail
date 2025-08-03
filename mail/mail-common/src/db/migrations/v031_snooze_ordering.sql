ALTER TABLE mail_conversation_scroll_data
RENAME COLUMN scroll_order TO order_dir;

ALTER TABLE mail_conversation_scroll_data
ADD COLUMN order_field INTEGER DEFAULT 0;

ALTER TABLE mail_message_scroll_data
RENAME COLUMN scroll_order TO order_dir;

ALTER TABLE mail_message_scroll_data
ADD COLUMN order_field INTEGER DEFAULT 0;

DELETE FROM mail_conversation_scroll_data
WHERE local_label_id IN (1, 14);
