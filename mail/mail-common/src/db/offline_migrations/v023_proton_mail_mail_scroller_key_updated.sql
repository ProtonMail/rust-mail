ALTER TABLE mail_conversation_scroll_data RENAME TO mail_conversation_scroll_data_old;

CREATE TABLE mail_conversation_scroll_data (
    local_label_id INTEGER NOT NULL,
    unread INTEGER NOT NULL DEFAULT 0,
    remote_conversation_id TEXT NOT NULL,
    conversation_time INTEGER NOT NULL,
    display_order INTEGER NOT NULL,
    scroll_order INTEGER NOT NULL,
    PRIMARY KEY (local_label_id, unread, scroll_order),
    CONSTRAINT local_label_id_mail_conversation_scroll_data FOREIGN KEY (local_label_id) REFERENCES labels (local_id) ON DELETE CASCADE
);

-- Schedule send needs to be refetched as the data was not stored with the correct ordering.
WITH schedule_send AS (
    SELECT local_id FROM labels WHERE remote_id = '12'
)
INSERT INTO mail_conversation_scroll_data (
    local_label_id,
    unread,
    remote_conversation_id,
    conversation_time,
    display_order,
    scroll_order
)
SELECT
    old.local_label_id,
    old.unread,
    old.remote_conversation_id,
    old.conversation_time,
    old.display_order,
    1
FROM mail_conversation_scroll_data_old AS old
WHERE old.local_label_id NOT IN (SELECT * FROM schedule_send);

DROP TABLE mail_conversation_scroll_data_old;

ALTER TABLE mail_message_scroll_data RENAME TO mail_message_scroll_data_old;

CREATE TABLE mail_message_scroll_data (
    local_label_id INTEGER NOT NULL,
    unread INTEGER NOT NULL DEFAULT 0,
    remote_message_id TEXT NOT NULL,
    message_time INTEGER NOT NULL,
    display_order INTEGER NOT NULL,
    scroll_order INTEGER NOT NULL,
    PRIMARY KEY (local_label_id, unread, scroll_order),
    CONSTRAINT local_label_id_mail_message_scroll_data FOREIGN KEY (local_label_id) REFERENCES labels (local_id) ON DELETE CASCADE
);

-- Schedule send needs to be refetched as the data was not stored with the correct ordering.
WITH schedule_send AS (
    SELECT local_id FROM labels WHERE remote_id = '12'
)
INSERT INTO mail_message_scroll_data (
    local_label_id,
    unread,
    remote_message_id,
    message_time,
    display_order,
    scroll_order
)
SELECT
    old.local_label_id,
    old.unread,
    old.remote_message_id,
    old.message_time,
    old.display_order,
    1
FROM mail_message_scroll_data_old AS old
WHERE old.local_label_id NOT IN (SELECT * FROM schedule_send);

DROP TABLE mail_message_scroll_data_old;