use stash::stash::{StashError, Tether};

pub async fn create_conversation_tables(tx: &Tether) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE conversations (
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE DEFAULT NULL,
                `order` INTEGER NOT NULL,
                subject TEXT NOT NULL,
                senders TEXT NOT NULL,
                recipients TEXT NOT NULL,
                num_messages INTEGER NOT NULL,
                num_unread INTEGER NOT NULL,
                num_attachments INTEGER NOT NULL,
                expiration_time INTEGER NOT NULL,
                size INTEGER NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                has_messages INTEGER NOT NULL DEFAULT 0
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE UNIQUE INDEX index_conversations_rid ON conversations (remote_id)",
        vec![],
    )
    .await?;

    // Conversation -> Labels
    tx.execute(
        r#"
            CREATE TABLE conversation_labels (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                local_conversation_id INTEGER NOT NULL,
                local_label_id INTEGER NOT NULL,
                ctx_time INTEGER NOT NULL,
                ctx_size INTEGER NOT NULL,
                ctx_num_messages INTEGER NOT NULL,
                ctx_num_unread INTEGER NOT NULL,
                ctx_num_attachments INTEGER NOT NULL,
                ctx_expiration_time INTEGER NOT NULL,
                ctx_snooze_time INTEGER NOT NULL,

                UNIQUE(local_conversation_id, local_label_id),

                CONSTRAINT constraint_conversation_labels_cid
                    FOREIGN KEY (local_conversation_id)
                    REFERENCES conversations (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE,

                CONSTRAINT constraint_conversation_labels_lid
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        r#"CREATE INDEX index_conversations_labels_cid ON conversation_labels (local_conversation_id)"#,
        vec![],
    ).await?;

    tx.execute(
        r#"CREATE INDEX index_conversations_labels_lid ON conversation_labels (local_label_id)"#,
        vec![],
    )
    .await?;

    //Conversation -> attachment
    tx.execute(
        r#"
            CREATE TABLE conversation_attachments(
                local_conversation_id INTEGER NOT NULL,
                local_attachment_id INTEGER NOT NULL,

                PRIMARY KEY(local_conversation_id, local_attachment_id),

                CONSTRAINT conversation_attachments_cid
                    FOREIGN KEY (local_conversation_id)
                    REFERENCES conversations (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE,

                CONSTRAINT conversation_attachments_aid
                    FOREIGN KEY (local_attachment_id)
                    REFERENCES attachments (local_id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        r#"CREATE INDEX index_conversations_attachments_cid ON conversation_attachments (local_conversation_id)"#,
        vec![],
    ).await?;
    tx.execute(
        r#"CREATE INDEX index_conversations_attachments_aid ON conversation_attachments (local_attachment_id)"#,
        vec![],
    ).await?;
    Ok(())
}
