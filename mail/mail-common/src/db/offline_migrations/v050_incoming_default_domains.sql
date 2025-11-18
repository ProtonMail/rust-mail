
ALTER TABLE incoming_defaults RENAME TO incoming_defaults_old;

CREATE TABLE incoming_defaults (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT DEFAULT NULL,

  location INTEGER NOT NULL,

  -- XOR
  email TEXT DEFAULT NULL, -- Changed: removed NOT NULL constraint
  domain TEXT DEFAULT NULL,

  deleted INTEGER DEFAULT 0 -- For soft deletion
);

-- Fixing table pluralization and adding local_id by autoincrement
INSERT INTO incoming_defaults (
    remote_id,
    email,
    location,
    domain
)
SELECT
    remote_id,
    email,
    location,
    domain
FROM incoming_defaults_old;

DROP TABLE incoming_defaults_old;
