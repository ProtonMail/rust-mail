-- Reset contact sync state to force a resync

DELETE FROM initialized_components
WHERE key = 'contacts' OR key = 'mail_user_context';
