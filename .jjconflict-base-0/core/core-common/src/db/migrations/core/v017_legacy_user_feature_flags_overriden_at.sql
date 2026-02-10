ALTER TABLE user_feature_flags RENAME TO old_user_feature_flags;

CREATE TABLE user_feature_flags (
    name TEXT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    source INTEGER NOT NULL DEFAULT 0, -- 0 Unleash, 1 Legacy
    writable BOOLEAN NOT NULL,
    overridden_to BOOLEAN,
    overridden_at INTEGER, -- Remote update at, set when the flag was overridden
    modify_time INTEGER NOT NULL,

    CHECK (
        (source = 0 AND writable = FALSE AND overridden_to IS NULL AND overridden_at IS NULL)
        OR
        (source = 1)
    )
);

INSERT INTO user_feature_flags (name, enabled, source, writable, overridden_to, modify_time)
SELECT name, enabled, source, writable, NULL, modify_time
FROM old_user_feature_flags;

DROP TABLE old_user_feature_flags;
