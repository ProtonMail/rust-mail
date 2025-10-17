
ALTER TABLE app_settings DROP COLUMN app_features;

CREATE TABLE feature_flags (
    name TEXT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    modify_time INTEGER NOT NULL
);
