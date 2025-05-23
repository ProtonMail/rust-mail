ALTER TABLE messages DROP CONSTRAINT messages_address_id;
ALTER TABLE messages ADD CONSTRAINT messages_address_id FOREIGN KEY (local_address_id) REFERENCES addresses (local_id) ON DELETE CASCADE;
