CREATE TABLE contacts (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT UNIQUE,
  name TEXT NOT NULL,
  uid TEXT NOT NULL,
  size INTEGER NOT NULL,
  create_time INTEGER NOT NULL,
  modify_time INTEGER NOT NULL,
  deleted INTEGER NOT NULL DEFAULT 0,
  label_ids TEXT NOT NULL
);

CREATE UNIQUE INDEX index_contact_remote_id ON contacts (remote_id);

CREATE TABLE contact_emails (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  remote_id TEXT UNIQUE,
  name TEXT NOT NULL,
  email TEXT NOT NULL COLLATE NOCASE,
  contact_type TEXT NOT NULL,
  defaults INTEGER NOT NULL,
  display_order INTEGER NOT NULL,
  local_contact_id INTEGER NOT NULL,
  remote_contact_id TEXT NOT NULL,
  label_ids TEXT NOT NULL,
  canonical_email TEXT NOT NULL,
  last_used_time INTEGER NOT NULL,
  is_proton INTEGER NOT NULL,
  CONSTRAINT constraint_contact_emails_local_cid FOREIGN KEY (local_contact_id) REFERENCES contacts (local_id) ON DELETE CASCADE CONSTRAINT constraint_contact_emails_remote_cid FOREIGN KEY (remote_contact_id) REFERENCES contacts (remote_id) ON DELETE CASCADE
);

CREATE INDEX index_contact_emails_email ON contact_emails (email);

CREATE INDEX index_contact_emails_contact_local_id ON contact_emails (local_contact_id);

CREATE INDEX index_contact_emails_contact_remote_id ON contact_emails (remote_contact_id);

CREATE TABLE contact_cards (
  local_id INTEGER PRIMARY KEY AUTOINCREMENT,
  local_contact_id INTEGER NOT NULL,
  remote_contact_id TEXT NOT NULL,
  card_type INTEGER NOT NULL,
  data TEXT NOT NULL,
  signature TEXT,
  CONSTRAINT constraint_contact_cards_local_cid FOREIGN KEY (local_contact_id) REFERENCES contacts (local_id) ON DELETE CASCADE CONSTRAINT constraint_contact_cards_remote_cid FOREIGN KEY (remote_contact_id) REFERENCES contacts (remote_id) ON DELETE CASCADE
);

CREATE INDEX index_contact_cards_local_id ON contact_cards (local_contact_id);

CREATE INDEX index_contact_cards_remote_id ON contact_cards (remote_contact_id);

CREATE TABLE contact_email_labels (
  contact_emails_id INTEGER NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (contact_emails_id, value),
  CONSTRAINT constraint_contact_label_cid FOREIGN KEY (contact_emails_id) REFERENCES contact_emails (local_id) ON DELETE CASCADE
);

CREATE INDEX index_contact_email_label_id ON contact_email_labels (contact_emails_id);
