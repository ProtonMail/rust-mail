UPDATE custom_settings
SET mobile_signature = replace(trim(mobile_signature), '\n', '<br />');
