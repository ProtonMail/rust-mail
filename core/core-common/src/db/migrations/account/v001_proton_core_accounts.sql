CREATE TABLE core_accounts (
  -- Remote ID of the account (i.e. the API User ID)
  remote_id TEXT PRIMARY KEY,
  -- The account's username or email address (used for login)
  name_or_addr TEXT NOT NULL,
  -- Whether the account is ready (i.e. login flow completed)
  is_ready INTEGER NOT NULL,
  -- Second factor auth mode of the account
  second_factor_mode INTEGER,
  -- Mailbox password mode of the account
  password_mode INTEGER,
  -- The account's username (once known)
  username TEXT,
  -- The account's password (encrypted, temporary)
  password BLOB,
  -- The account's display name (once known)
  display_name TEXT,
  -- The account's primary email address (once known)
  primary_addr TEXT,
  -- Timestamp of when account was made primary
  primary_at REAL
);

CREATE UNIQUE INDEX index_core_accounts_remote_id ON core_accounts (remote_id)
