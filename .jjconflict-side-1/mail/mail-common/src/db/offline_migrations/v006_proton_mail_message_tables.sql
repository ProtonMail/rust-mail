CREATE TABLE messages (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT UNIQUE DEFAULT NULL,
  local_address_id INTEGER NOT NULL,
  remote_address_id TEXT NOT NULL,
  local_conversation_id INTEGER NOT NULL,
  remote_conversation_id TEXT DEFAULT NULL,
  display_order INTEGER NOT NULL,
  subject TEXT NOT NULL,
  unread INTEGER NOT NULL,
  to_list TEXT DEFAULT NULL,
  cc_list TEXT DEFAULT NULL,
  bcc_list TEXT DEFAULT NULL,
  reply_tos TEXT DEFAULT NULL,
  sender TEXT DEFAULT NULL,
  time INTEGER NOT NULL,
  size INTEGER NOT NULL,
  expiration_time INTEGER NOT NULL,
  is_replied INTEGER NOT NULL,
  is_replied_all INTEGER NOT NULL,
  is_forwarded INTEGER NOT NULL,
  external_id TEXT,
  num_attachments INTEGER NOT NULL,
  flags INTEGER NOT NULL,
  snooze_time INTEGER NOT NULL DEFAULT 0,
  deleted INTEGER NOT NULL DEFAULT 0,
  CONSTRAINT messages_address_id FOREIGN KEY (local_address_id) REFERENCES addresses (local_id),
  CONSTRAINT messages_conversation_id FOREIGN KEY (local_conversation_id) REFERENCES conversations (local_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX index_messages_rid ON messages (remote_id);

CREATE INDEX index_messages_cid ON messages (local_conversation_id);

CREATE INDEX index_messages_conv_rid ON messages (remote_conversation_id);

-- Message -> Labels
CREATE TABLE message_labels (
  local_message_id INTEGER NOT NULL,
  local_label_id INTEGER NOT NULL,
  PRIMARY KEY (local_message_id, local_label_id),
  CONSTRAINT message_labels_mid FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT message_labels_lid FOREIGN KEY (local_label_id) REFERENCES labels (local_id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX index_messages_labels_mid ON message_labels (local_message_id);

CREATE INDEX index_messages_labels_lid ON message_labels (local_label_id);

-- Messages -> Attachment
CREATE TABLE message_attachments (
  local_message_id INTEGER NOT NULL,
  local_attachment_id INTEGER NOT NULL,
  PRIMARY KEY (local_message_id, local_attachment_id),
  CONSTRAINT message_attachments_cid FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT message_attachments_aid FOREIGN KEY (local_attachment_id) REFERENCES attachments (local_id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX index_messages_attachments_cid ON message_attachments (local_message_id);

CREATE INDEX index_messages_attachments_aid ON message_attachments (local_attachment_id);

CREATE TABLE message_bodies (
  local_message_id INTEGER PRIMARY KEY NOT NULL,
  remote_message_id TEXT UNIQUE DEFAULT NULL,
  header TEXT NOT NULL,
  parsed_headers TEXT NOT NULL,
  mime_type INTEGER NOT NULL,
  CONSTRAINT message_bodies_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

CREATE TABLE draft_metadata (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  local_message_id INTEGER DEFAULT NULL UNIQUE,
  local_conversation_id INTEGER DEFAULT NULL,
  local_parent_id INTEGER DEFAULT NULL,
  reply_mode INTEGER DEFAULT NULL,
  save_action_id INTEGER DEFAULT NULL,
  send_action_id INTEGER DEFAULT NULL,
  CONSTRAINT create_draft_metadata_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE CONSTRAINT create_draft_metadata_conversation_id FOREIGN KEY (local_conversation_id) REFERENCES conversations (local_id) ON DELETE SET NULL CONSTRAINT create_draft_metadata_parent_id FOREIGN KEY (local_parent_id) REFERENCES messages (local_id) ON DELETE SET NULL CONSTRAINT draft_metadata_save_action_id FOREIGN KEY (save_action_id) REFERENCES action_queue (id) ON DELETE SET NULL CONSTRAINT draft_metadata_send_action_id FOREIGN KEY (send_action_id) REFERENCES action_queue (id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX index_draft_metadatqa_mid ON draft_metadata (local_message_id);

CREATE TABLE draft_send_result (
  local_message_id INTEGER PRIMARY KEY,
  remote_message_id TEXT DEFAULT NULL,
  timestamp INTEGER NOT NULL DEFAULT (now ()),
  undo_timestamp INTEGER NOT NULL,
  error TEXT DEFAULT NULL,
  seen INTEGER NOT NULL DEFAULT 0,
  origin INTEGER NOT NULL,
  CONSTRAINT draft_send_result_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

-- Draft Attachment Upload metadata
CREATE TABLE draft_attachment_metadata (
  local_attachment_id INTEGER PRIMARY KEY,
  metadata_id INTEGER NOT NULL,
  timestamp INTEGER NOT NULL DEFAULT (now ()),
  state INTEGER NOT NULL,
  error TEXT DEFAULT NULL,
  action_id INTEGER DEFAULT NULL,
  display_order INTEGER NOT NULL DEFAULT 0,
  ownership INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0,
  CONSTRAINT draft_attachment_metadata_attachment_id FOREIGN KEY (local_attachment_id) REFERENCES attachments (local_id) ON DELETE CASCADE CONSTRAINT draft_attachment_metadata_metadata_id FOREIGN KEY (metadata_id) REFERENCES draft_metadata (id) ON DELETE CASCADE CONSTRAINT draft_attachment_metadata_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE SET NULL
);

CREATE TABLE message_body (
    message_id INTEGER PRIMARY KEY,
    body TEXT NOT NULL,

CONSTRAINT message_body_id
    FOREIGN KEY (message_id)
    REFERENCES messages (local_id)
    ON DELETE CASCADE
);
