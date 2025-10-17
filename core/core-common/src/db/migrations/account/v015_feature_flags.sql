
ALTER TABLE app_settings DROP COLUMN app_features;

CREATE TABLE feature_flags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    modify_time INTEGER NOT NULL
);
