UPDATE mail_conversation_scroll_data
SET order_field =1
WHERE local_label_id IN (SELECT local_label_id
                         FROM labels
                         WHERE remote_id IN ('0', '16'));

UPDATE mail_message_scroll_data
SET order_field =1
WHERE local_label_id IN (SELECT local_label_id
                         FROM labels
                         WHERE remote_id IN ('0', '16'));
