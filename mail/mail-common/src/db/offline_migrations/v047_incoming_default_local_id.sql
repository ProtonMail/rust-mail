
CREATE TABLE incoming_defaults (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT DEFAULT NULL,

  email TEXT NOT NULL,
  location INTEGER NOT NULL,

  domain TEXT DEFAULT NULL, -- unused for now

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
    id,
    email,
    location,
    domain
FROM incoming_default
WHERE email IS NOT NULL;

DROP TABLE incoming_default;
