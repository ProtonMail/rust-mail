CREATE TABLE deleted_items (
  remote_id TEXT NOT NULL,
  item_type INTEGER NOT NULL,
  deleted_at INTEGER NOT NULL,
  PRIMARY KEY (remote_id, item_type)
);
