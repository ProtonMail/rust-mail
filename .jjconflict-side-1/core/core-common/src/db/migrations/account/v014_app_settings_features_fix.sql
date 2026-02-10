UPDATE app_settings
SET app_features = json_extract(app_features, '$.features')
WHERE json_extract(app_features, '$.features') IS NOT NULL;
