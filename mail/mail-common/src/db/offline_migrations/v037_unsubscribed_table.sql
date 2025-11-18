CREATE TABLE unsubscribe (
  local_message_id INTEGER PRIMARY KEY,
  CONSTRAINT unsubscribe_local_id FOREIGN KEY (local_message_id) REFERENCES messages (local_id) ON DELETE CASCADE
)
