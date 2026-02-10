ALTER TABLE addresses ADD COLUMN flags INTEGER DEFAULT NULL;
INSERT INTO pending_online_migrations (name) VALUES ('fetch-address-flags');
