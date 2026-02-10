CREATE TABLE incoming_default (
  local_address_id INTEGER NOT NULL,
  location INTEGER NOT NULL,

  CONSTRAINT incoming_default_id_references_local_address_id
      FOREIGN KEY (local_address_id)
      REFERENCES addresses (local_id)
      ON DELETE CASCADE
);

