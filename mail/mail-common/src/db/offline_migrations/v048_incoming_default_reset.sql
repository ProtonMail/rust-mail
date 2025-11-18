-- by deleting those two keys we are enforcing partial re-initialization.
-- We do it because we added email sanitization and we want to re-fetch incoming defaults from the API.
delete from initialized_components where key = 'incoming_defaults' OR key = 'mail_user_context';
