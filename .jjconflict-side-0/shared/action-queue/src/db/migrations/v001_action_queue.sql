CREATE TABLE action_queue (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `action_type` TEXT NOT NULL,
  version INTEGER NOT NULL,
  priority INTEGER NOT NULL,
  created INTEGER DEFAULT (datetime ('now')),
  scheduled INTEGER DEFAULT (datetime ('now')),
  state BLOB NOT NULL,
  debug_string TEXT DEFAULT NULL,
  action_group TEXT NOT NULL
);

CREATE INDEX action_queue_idx_prio ON action_queue (priority);

CREATE INDEX action_queue_idx_date ON action_queue (created);

CREATE INDEX action_queue_idx_delay ON action_queue (scheduled);

CREATE TABLE action_queue_dependencies (
  action_id INTEGER NOT NULL,
  dependency_id INTEGER NOT NULL,
  PRIMARY KEY (action_id, dependency_id),
  CONSTRAINT action_queue_dep_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE CASCADE,
  CONSTRAINT action_queue_dep_dep_id FOREIGN KEY (dependency_id) REFERENCES action_queue (id) ON DELETE CASCADE
);

CREATE TABLE action_queue_resources (
  action_id INTEGER PRIMARY KEY,
  resource BLOB NOT NULL,
  CONSTRAINT action_queue_res_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE CASCADE
);

-- Create execution Lock Table - This is kept separate from the action
-- to prevent accidental overrides via the Model::save methods
CREATE TABLE action_queue_lock (
  action_id INTEGER PRIMARY KEY,
  executor_id TEXT UNIQUE DEFAULT NULL,
  acquired_at INTEGER NOT NULL DEFAULT 0,
  permit_id INTEGER NOT NULL DEFAULT 0,
  CONSTRAINT action_queue_lock_action_id FOREIGN KEY (action_id) REFERENCES action_queue (id) ON DELETE CASCADE
);

