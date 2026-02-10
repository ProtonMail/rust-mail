CREATE TABLE addresses (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT NOT NULL,
  domain_id TEXT DEFAULT NULL,
  email TEXT UNIQUE NOT NULL,
  send INTEGER NOT NULL,
  receive INTEGER NOT NULL,
  status INTEGER NOT NULL,
  address_type INTEGER NOT NULL,
  display_order INTEGER NOT NULL,
  display_name TEXT NOT NULL,
  signature TEXT NOT NULL,
  catch_all INTEGER NOT NULL,
  proton_mx INTEGER NOT NULL,
  signed_key_list TEXT,
  keys TEXT
);

CREATE UNIQUE INDEX index_addresses_email ON addresses (email);

CREATE TABLE address_keys (
  remote_id TEXT PRIMARY KEY,
  address_id INTEGER NOT NULL,
  version INTEGER NOT NULL,
  private_key TEXT,
  token TEXT,
  signature TEXT,
  is_primary INTEGER NOT NULL,
  is_active INTEGER NOT NULL,
  flags INTEGER,
  address_forwarding_id TEXT,
  CONSTRAINT address_keys_id FOREIGN KEY (address_id) REFERENCES addresses (local_id) ON DELETE CASCADE,
  CONSTRAINT address_keys_forwarding_id FOREIGN KEY (address_forwarding_id) REFERENCES addresses (local_id) ON DELETE SET NULL
);

CREATE INDEX index_address_keys_addr_id ON address_keys (address_id)
