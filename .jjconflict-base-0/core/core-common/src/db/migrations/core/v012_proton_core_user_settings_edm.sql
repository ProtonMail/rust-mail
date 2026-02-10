UPDATE user_settings
SET flags = json_set(flags, '$.edm_opt_out', json('false'))
WHERE json_extract(flags, '$.edm_opt_out') = 0;
