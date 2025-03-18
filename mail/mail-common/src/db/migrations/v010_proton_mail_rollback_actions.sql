CREATE TABLE rollback_actions (
  remote_id TEXT NOT NULL,
  item_type INTEGER NOT NULL,
  PRIMARY KEY (remote_id, item_type)
);

CREATE INDEX index_rollback_actions_item_type ON rollback_actions (item_type)
