-- Initialize contact and mail events with core event id

INSERT OR IGNORE INTO event_id_store
SELECT 'proton-contact-event', value
FROM event_id_store
WHERE id == 'proton-core-event';

INSERT OR IGNORE INTO event_id_store
SELECT 'proton-mail-event', value
FROM event_id_store
WHERE id == 'proton-core-event';
