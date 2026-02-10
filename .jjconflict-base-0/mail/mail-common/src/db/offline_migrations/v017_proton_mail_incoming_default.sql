DROP TABLE IF EXISTS incoming_default;

CREATE TABLE incoming_default (
  email TEXT DEFAULT NULL,
  location INTEGER,
  id TEXT DEFAULT NULL,

  domain TEXT DEFAULT NULL -- unused for now
);
