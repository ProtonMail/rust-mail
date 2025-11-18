-- Trigger is a substitution of Foreign key `ON DELETE CASCASE`
-- The reason why the trigger was createad is that sqlite do not support
-- Altering CONSTRAINs and if we would like to add this constrain to `messages`
-- table we would have to drop all the data from this table as foreign keys
-- would point to the old table instead the new one with redefined Foreign key.
CREATE TRIGGER cascade_delete_messages_on_addresses_deletion
BEFORE DELETE ON addresses
FOR EACH ROW
BEGIN
    DELETE FROM messages WHERE local_address_id = OLD.local_id;
END;
