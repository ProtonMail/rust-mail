INSERT OR IGNORE INTO labels (remote_id, label_type, name, color, display_order)
VALUES (13, 4, 'Broken', '#000000', 13);

INSERT OR IGNORE INTO message_counters (local_label_id, total, unread)
VALUES ((SELECT local_id FROM labels WHERE remote_id = '13'), 0, 0);

INSERT OR IGNORE INTO conversation_counters (local_label_id, total, unread)
VALUES ((SELECT local_id FROM labels WHERE remote_id = '13'), 0, 0);
