-- There was a bug in the previous build that caused the v009 migration to be skipped.
-- Run it again as part of v10 to correct invalid state.
UPDATE user_settings
SET flags = json_set(flags, '$.edm_opt_out', false)
WHERE json_extract(flags, '$.edm_opt_out') IS NULL;
