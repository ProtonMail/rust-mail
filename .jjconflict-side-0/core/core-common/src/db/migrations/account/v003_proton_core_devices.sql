CREATE TABLE registered_devices (
  device_token TEXT NOT NULL PRIMARY KEY UNIQUE,
  environment INTEGER NOT NULL,
  public_key TEXT DEFAULT NULL,
  ping_notification_status INTEGER DEFAULT NULL,
  push_notification_status INTEGER DEFAULT NULL
);

CREATE TRIGGER registered_devices_only_one_row BEFORE INSERT ON registered_devices WHEN (
  SELECT
    COUNT(*)
  FROM
    registered_devices
) >= 1 BEGIN
SELECT
  RAISE (
    FAIL,
    'registered_devices may have only one row. This is a bug in a model layer'
  );

END
