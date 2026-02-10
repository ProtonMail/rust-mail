CREATE TABLE conversations (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT UNIQUE DEFAULT NULL,
  display_order INTEGER NOT NULL,
  subject TEXT NOT NULL,
  senders TEXT DEFAULT NULL,
  recipients TEXT DEFAULT NULL,
  num_messages INTEGER NOT NULL,
  num_unread INTEGER NOT NULL,
  num_attachments INTEGER NOT NULL,
  attachment_info TEXT DEFAULT NULL,
  expiration_time INTEGER NOT NULL,
  size INTEGER NOT NULL,
  display_snooze_reminder INTEGER NOT NULL DEFAULT 0,
  deleted INTEGER NOT NULL DEFAULT 0,
  is_known INTEGER NOT NULL,
  has_messages INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX index_conversations_rid ON conversations (remote_id);

-- Conversation -> Labels
CREATE TABLE conversation_labels (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  local_conversation_id INTEGER NOT NULL,
  local_label_id INTEGER NOT NULL,
  remote_label_id TEXT DEFAULT NULL,
  context_time INTEGER NOT NULL,
  context_size INTEGER NOT NULL,
  context_num_messages INTEGER NOT NULL,
  context_num_unread INTEGER NOT NULL,
  context_num_attachments INTEGER NOT NULL,
  context_expiration_time INTEGER NOT NULL,
  context_snooze_time INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0,
  UNIQUE (local_conversation_id, local_label_id),
  CONSTRAINT constraint_conversation_labels_cid FOREIGN KEY (local_conversation_id) REFERENCES conversations (local_id) ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT constraint_conversation_labels_lid FOREIGN KEY (local_label_id) REFERENCES labels (local_id) ON DELETE CASCADE
);

CREATE INDEX index_conversations_labels_cid ON conversation_labels (local_conversation_id);

CREATE INDEX index_conversations_labels_lid ON conversation_labels (local_label_id);

-- Conversation -> Attachment
CREATE TABLE conversation_attachments (
  local_conversation_id INTEGER NOT NULL,
  local_attachment_id INTEGER NOT NULL,
  PRIMARY KEY (local_conversation_id, local_attachment_id),
  CONSTRAINT conversation_attachments_cid FOREIGN KEY (local_conversation_id) REFERENCES conversations (local_id) ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT conversation_attachments_aid FOREIGN KEY (local_attachment_id) REFERENCES attachments (local_id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE INDEX index_conversations_attachments_cid ON conversation_attachments (local_conversation_id);

CREATE INDEX index_conversations_attachments_aid ON conversation_attachments (local_attachment_id);
