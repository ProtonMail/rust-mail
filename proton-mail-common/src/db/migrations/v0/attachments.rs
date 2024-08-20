use indoc::indoc;
use stash::stash::{Interface, StashError, Tether};

pub async fn create_attachment_tables(tx: &Tether) -> Result<(), StashError> {
    // Attachments
    tx.execute(
        indoc! {"
            CREATE TABLE attachments (
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE DEFAULT NULL,
                local_conversation_id INTEGER DEFAULT NULL,
                remote_conversation_id TEXT DEFAULT NULL,
                local_message_id INTEGER DEFAULT NULL,
                remote_message_id TEXT DEFAULT NULL,
                filename TEXT NOT NULL,
                size INTEGER NOT NULL,
                mime_type INTEGER NOT NULL,
                local_address_id INTEGER DEFAULT NULL,
                remote_address_id TEXT DEFAULT NULL,
                key_packets TEXT DEFAULT NULL,
                signature TEXT DEFAULT NULL,
                enc_signature TEXT DEFAULT NULL,
                disposition INTEGER NOT NULL,
                sender TEXT DEFAULT NULL,
                is_auto_forwardee INTEGER NOT NULL DEFAULT 0,
                content_id TEXT DEFAULT NULL,
                transfer_encoding TEXT DEFAULT NULL,
                image_width TEXT DEFAULT NULL,
                image_height TEXT DEFAULT NULL,

                CONSTRAINT attachments_address_id
                    FOREIGN KEY (local_address_id)
                    REFERENCES addresses (local_id),

                CONSTRAINT attachments_conversation_id
                    FOREIGN KEY (local_conversation_id)
                    REFERENCES conversations (local_id)
                    ON DELETE CASCADE,

                CONSTRAINT attachments_message_id
                    FOREIGN KEY (local_message_id)
                    REFERENCES messages (local_id)
                    ON DELETE CASCADE
            )
        "},
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE UNIQUE INDEX index_attachments_rid ON attachments (remote_id)",
        vec![],
    )
    .await?;
    Ok(())
}
