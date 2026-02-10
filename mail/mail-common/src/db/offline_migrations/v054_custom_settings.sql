ALTER TABLE custom_settings ADD COLUMN mobile_signature_new TEXT DEFAULT '';

UPDATE custom_settings SET mobile_signature_new = mobile_signature;

ALTER TABLE custom_settings DROP COLUMN mobile_signature;
ALTER TABLE custom_settings RENAME COLUMN mobile_signature_new TO mobile_signature;
