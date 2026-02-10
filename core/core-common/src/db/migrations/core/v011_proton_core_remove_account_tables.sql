-- Remove account tables which were accidentally added in a previous commit.
DROP TABLE IF EXISTS core_accounts;
DROP TABLE IF EXISTS core_sessions;
DROP TABLE IF EXISTS app_settings;
DROP TABLE IF EXISTS pin_protection;
