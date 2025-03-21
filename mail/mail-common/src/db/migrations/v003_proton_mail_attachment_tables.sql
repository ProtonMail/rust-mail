CREATE TABLE attachments (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  local_conversation_id INTEGER DEFAULT NULL,
  remote_conversation_id TEXT DEFAULT NULL,
  local_message_id INTEGER DEFAULT NULL,
  remote_message_id TEXT DEFAULT NULL,
  filename TEXT NOT NULL,
  size INTEGER NOT NULL,
  mime_type INTEGER NOT NULL,
  local_address_id INTEGER DEFAULT NULL,
  remote_address_id TEXT DEFAULT NULL,
  key_packets TEXT DEFAULT NULL,
  signature TEXT DEFAULT NULL,
  enc_signature TEXT DEFAULT NULL,
  disposition INTEGER NOT NULL,
  sender TEXT DEFAULT NULL,
  is_auto_forwardee INTEGER NOT NULL DEFAULT 0,
  content_id TEXT DEFAULT NULL,
  transfer_encoding TEXT DEFAULT NULL,
  image_width TEXT DEFAULT NULL,
  image_height TEXT DEFAULT NULL,
  attachment_type TEXT NOT NULL, -- JSON

  CONSTRAINT attachments_address_id FOREIGN KEY (local_address_id) REFERENCES addresses (local_id),
  CONSTRAINT attachments_conversation_id FOREIGN KEY (local_conversation_id) REFERENCES conversations (local_id) ON DELETE CASCADE,
  CONSTRAINT attachments_message_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
);

CREATE TABLE attachment_cache (
    attachment_id INTEGER PRIMARY KEY,
    atime INTEGER NOT NULL DEFAULT (unixepoch('now')),
    ctime INTEGER NOT NULL DEFAULT (unixepoch('now')),
    hit_count INTEGER NOT NULL DEFAULT 0,
    path TEXT NOT NULL,
    size INTEGER NOT NULL
);
