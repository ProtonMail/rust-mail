-- Unfortunately adding CHECK is possible only with a new table.
ALTER TABLE user_feature_flags RENAME TO old_user_feature_flags;

CREATE TABLE user_feature_flags (
    name TEXT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    source INTEGER NOT NULL DEFAULT 0, -- 0 Unleash, 1 Legacy
    writable BOOLEAN NOT NULL,
    override BOOLEAN,
    modify_time INTEGER NOT NULL,

    CHECK (
        (source = 0 AND writable = FALSE AND override IS NULL)
        OR
        (source = 1)
    )
);

INSERT INTO user_feature_flags (name, enabled, source, writable, override, modify_time)
SELECT name, enabled, 0, FALSE, NULL, modify_time
FROM old_user_feature_flags;

DROP TABLE old_user_feature_flags;
