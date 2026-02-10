-- Remove mobile_settings column as it somehow
-- was defined as integer (and was working in most cases!)
ALTER TABLE mail_settings
    DROP COLUMN mobile_settings;

ALTER TABLE mail_settings
    ADD COLUMN mobile_settings TEXT DEFAULT NULL;

-- Make it initialize again for refetching mobile settings from the backend
UPDATE initialized_components
SET state = 0
WHERE key IN ('mail_settings', 'mail_user_context');
