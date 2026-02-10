CREATE TABLE user_feature_flags (
    name TEXT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    modify_time INTEGER NOT NULL
);
