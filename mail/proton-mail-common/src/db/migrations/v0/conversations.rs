use proton_sqlite3::rusqlite::Transaction;

pub fn create_conversation_tables(tx: &mut Transaction) -> crate::db::DBResult<()> {
    tx.execute(
        r#"
            CREATE TABLE conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rid TEXT UNIQUE,
                `order` INTEGER NOT NULL,
                subject TEXT NOT NULL,
                senders TEXT NOT NULL,
                recipients TEXT NOT NULL,
                num_messages INTEGER NOT NULL,
                num_unread INTEGER NOT NULL,
                num_attachments INTEGER NOT NULL,
                expiration_time INTEGER NOT NULL,
                size INTEGER NOT NULL,
                flagged INTEGER NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0
            )
        "#,
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_conversations_rid ON conversations (rid)",
        (),
    )?;

    // Conversation -> Labels
    tx.execute(
        r#"
            CREATE TABLE conversation_labels (
               conversation_id INTEGER NOT NULL,
               label_id INTEGER NOT NULL,
               ctx_time INTEGER NOT NULL,
               ctx_size INTEGER NOT NULL,
               ctx_num_messages INTEGER NOT NULL,
               ctx_num_unread INTEGER NOT NULL,
               ctx_num_attachments INTEGER NOT NULL,
               ctx_expiration_time INTEGER NOT NULL,
               
               PRIMARY KEY(conversation_id, label_id),
               
               CONSTRAINT constraint_conversation_labels_cid
                   FOREIGN KEY (conversation_id)
                   REFERENCES conversations (id)
                   ON DELETE CASCADE ON UPDATE CASCADE,
               
               CONSTRAINT constraint_conversation_labels_lid
                   FOREIGN KEY (label_id)
                   REFERENCES labels (id)
                   ON DELETE CASCADE
            )
        "#,
        (),
    )?;

    tx.execute(
        r#"CREATE INDEX index_conversations_labes_cid ON conversation_labels (conversation_id)"#,
        (),
    )?;

    tx.execute(
        r#"CREATE INDEX index_conversations_labes_lid ON conversation_labels (label_id)"#,
        (),
    )?;

    //Conversation -> attachment
    tx.execute(
        r#"
            CREATE TABLE conversation_attachments(
                conversation_id INTEGER NOT NULL,
                attachment_id INTEGER NOT NULL,
                
                PRIMARY KEY(conversation_id, attachment_id),
                
                CONSTRAINT conversation_attachments_cid
                    FOREIGN KEY (conversation_id)
                    REFERENCES conversations (id)
                    ON DELETE CASCADE ON UPDATE CASCADE,
                
                CONSTRAINT conversation_attachments_aid
                    FOREIGN KEY (attachment_id)
                    REFERENCES attachments (id)
                    ON DELETE CASCADE ON UPDATE CASCADE
            )
        "#,
        (),
    )?;

    tx.execute(
        r#"CREATE INDEX index_conversations_attachments_cid ON conversation_attachments (conversation_id)"#,
        (),
    )?;
    tx.execute(
        r#"CREATE INDEX index_conversations_attachments_aid ON conversation_attachments (attachment_id)"#,
        (),
    )?;

    Ok(())
}
