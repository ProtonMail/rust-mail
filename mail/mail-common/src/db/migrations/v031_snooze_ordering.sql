ALTER TABLE mail_conversation_scroll_data
RENAME COLUMN scroll_order TO order_dir;

ALTER TABLE mail_message_scroll_data
RENAME COLUMN scroll_order TO order_dir;

DELETE FROM mail_conversation_scroll_data
WHERE local_label_id IN (1, 14);
