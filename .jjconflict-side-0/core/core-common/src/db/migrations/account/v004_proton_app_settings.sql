CREATE TABLE app_settings(
  local_id INTEGER PRIMARY KEY,
  appearance INTEGER NOT NULL,
  protection INTEGER NOT NULL,
  auto_lock INTEGER NOT NULL,
  use_combine_contacts INTEGER NOT NULL,
  use_alternative_routing INTEGER NOT NULL
);

CREATE TABLE pin_protection(
  local_id INTEGER PRIMARY KEY,
  attempts INTEGER NOT NULL,
  last_access_unixepoch INTEGER NOT NULL
);
