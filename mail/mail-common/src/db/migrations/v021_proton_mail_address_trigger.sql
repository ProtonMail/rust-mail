CREATE TRIGGER cascade_delete_messages_on_addresses_deletion
BEFORE DELETE ON addresses
FOR EACH ROW
BEGIN
    DELETE FROM messages WHERE local_address_id = OLD.local_id;
END;
