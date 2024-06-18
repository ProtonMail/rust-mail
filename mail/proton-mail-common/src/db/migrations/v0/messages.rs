use indoc::indoc;
use stash::stash::{StashError, Tether};

pub async fn create_message_tables(tx: &Tether) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE messages(
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE DEFAULT NULL,
                address_id TEXT NOT NULL,
                local_conversation_id INTEGER DEFAULT NULL,
                remote_conversation_id TEXT DEFAULT NULL,
                `order` INTEGER NOT NULL,
                subject TEXT NOT NULL,
                unread INTEGER NOT NULL,
                sender_address TEXT NOT NULL,
                sender_name TEXT NOT NULL,
                sender_is_proton INTEGER NOT NULL DEFAULT 0,
                sender_is_simple_login INTEGER NOT NULL DEFAULT 0,
                sender_bimi_selector TEXT DEFAULT NULL,
                sender_display_image INTEGER NOT NULL DEFAULT 0,
                to_list TEXT DEFAULT NULL,
                cc_list TEXT DEFAULT NULL,
                bcc_list TEXT DEFAULT NULL,
                time INTEGER NOT NULL,
                size INTEGER NOT NULL,
                expiration_time INTEGER NOT NULL,
                is_replied INTEGER NOT NULL,
                is_replied_all INTEGER NOT NULL,
                is_forwarded INTEGER NOT NULL,
                external_id TEXT,
                num_attachments INTEGER NOT NULL,
                flags INTEGER NOT NULL,
                snooze_time INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,

                CONSTRAINT messages_address_id
                    FOREIGN KEY (address_id)
                    REFERENCES addresses (id),

                CONSTRAINT messages_conversation_id
                    FOREIGN KEY (local_conversation_id)
                    REFERENCES conversations (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE UNIQUE INDEX index_messages_rid ON messages (remote_id)",
        vec![],
    )
    .await?;
    tx.execute(
        "CREATE INDEX index_messages_cid ON messages (local_conversation_id)",
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE INDEX index_messages_conv_rid ON messages (remote_conversation_id)",
        vec![],
    )
    .await?;

    //message -> labels
    tx.execute(
        r#"
            CREATE TABLE message_labels(
                local_message_id INTEGER NOT NULL,
                local_label_id INTEGER NOT NULL,

                PRIMARY KEY(local_message_id, local_label_id),

                CONSTRAINT message_labels_mid
                    FOREIGN KEY (local_message_id)
                    REFERENCES messages (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE,

                CONSTRAINT message_labels_lid
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        r#"CREATE INDEX index_messages_labels_mid ON message_labels (local_message_id)"#,
        vec![],
    )
    .await?;
    tx.execute(
        r#"CREATE INDEX index_messages_labels_lid ON message_labels(local_label_id)"#,
        vec![],
    )
    .await?;

    //messages -> attachment
    tx.execute(
        r#"
            CREATE TABLE message_attachments(
                local_message_id INTEGER NOT NULL,
                local_attachment_id INTEGER NOT NULL,

                PRIMARY KEY(local_message_id, local_attachment_id),

                CONSTRAINT message_attachments_cid
                    FOREIGN KEY (local_message_id)
                    REFERENCES messages (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE,

                CONSTRAINT message_attachments_aid
                    FOREIGN KEY (local_attachment_id)
                    REFERENCES attachments (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        r#"CREATE INDEX index_messages_attachments_cid ON message_attachments (local_message_id)"#,
        vec![],
    )
    .await?;
    tx.execute(
        r#"CREATE INDEX index_messages_attachments_aid ON message_attachments (local_attachment_id)"#,
        vec![],
    ).await?;

    // Message bodies table
    tx.execute(
        indoc! {"
        CREATE TABLE message_bodies (
            local_message_id INTEGER PRIMARY KEY NOT NULL,
            header TEXT NOT NULL,
            parsed_headers TEXT NOT NULL,
            mime_type TEXT NOT NULL,

            CONSTRAINT message_bodies_id
                FOREIGN KEY (local_message_id)
                REFERENCES messages (local_id)
                ON DELETE CASCADE
        )"
        },
        vec![],
    )
    .await;
    Ok(())
}
