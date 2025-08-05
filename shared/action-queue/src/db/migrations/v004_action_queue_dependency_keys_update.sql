CREATE TABLE action_queue_key_deps_v2
(
    key_id    TEXT KEY,
    action_id INTEGER NOT NULL,
    CONSTRAINT action_queue_key_deps_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE CASCADE
);

CREATE INDEX action_queue_key_deps_v2_key_id ON action_queue_key_deps_v2 (key_id);


INSERT INTO action_queue_key_deps_v2
SELECT *
FROM action_queue_key_deps;

DROP TABLE action_queue_key_deps;
