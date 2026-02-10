-- Remove fido_details column from core_sessions table
-- FIDO2 details should not be persisted as they are single-use challenges
ALTER TABLE core_sessions DROP COLUMN fido_details;
