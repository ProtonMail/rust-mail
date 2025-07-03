CREATE TABLE action_queue_key_deps
(
    key_id    TEXT PRIMARY KEY,
    action_id INTEGER NOT NULL,
    CONSTRAINT action_queue_key_deps_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE CASCADE
);