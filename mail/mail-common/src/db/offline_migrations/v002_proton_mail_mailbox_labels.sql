CREATE TABLE mailbox_labels (
  local_label_id INTEGER PRIMARY KEY,
  initialized INTEGER NOT NULL DEFAULT 0,
  CONSTRAINT create_mailbox_labels_label_id FOREIGN KEY (local_label_id) REFERENCES labels (local_id) ON DELETE CASCADE
)
