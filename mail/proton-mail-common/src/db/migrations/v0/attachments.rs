use indoc::indoc;
use proton_sqlite3::SqliteTransaction;

pub fn create_attachment_tables(tx: &mut SqliteTransaction) -> crate::db::DBResult<()> {
    // Attachments
    tx.execute(
        indoc! {"
            CREATE TABLE attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rid TEXT UNIQUE,
                name TEXT NON NULL,
                size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                address_id TEXT DEFAULT NULL,
                key_packets TEXT DEFAULT NULL,
                signature TEXT DEFAULT NULL,
                enc_signature TEXT DEFAULT NULL,
                disposition TEXT NOT NULL,
                sender TEXT DEFAULT NULL,
                conversation_id INTEGER DEFAULT NULL,
                message_id INTEGER DEFAULT NULL,
                is_auto_forwardee INTEGER NOT NULL DEFAULT 0,
                content_id TEXT DEFAULT NULL,
                transfer_encoding TEXT DEFAULT NULL,
                image_width TEXT DEFAULT NULL,
                image_height TEXT DEFAULT NULL,

                CONSTRAINT attachments_address_id
                    FOREIGN KEY (address_id)
                    REFERENCES addresses (id),

                CONSTRAINT attachments_conversation_id
                    FOREIGN KEY (conversation_id)
                    REFERENCES conversations (id)
                    ON DELETE CASCADE,

                CONSTRAINT attachments_message_id
                    FOREIGN KEY (message_id)
                    REFERENCES messages (id)
                    ON DELETE CASCADE
            )
        "},
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_attachments_rid ON attachments (rid)",
        (),
    )?;

    Ok(())
}
