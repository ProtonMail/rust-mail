CREATE TABLE core_sessions (
  -- Remote ID of the session (i.e. the API Auth UID)
  remote_id TEXT PRIMARY KEY,
  -- Account ID the session is associated with (i.e. the API User ID)
  account_id TEXT NOT NULL REFERENCES core_accounts (remote_id) ON DELETE CASCADE,
  -- Access token for the session
  access_token BLOB NOT NULL,
  -- Refresh token for the session
  refresh_token BLOB NOT NULL,
  -- The API scope(s) the session has access to
  auth_scopes TEXT NOT NULL,
  -- Secret used for unlocking the PGP key(s)
  key_secret BLOB
);

CREATE UNIQUE INDEX index_core_sessions_remote_id ON core_sessions (remote_id)
