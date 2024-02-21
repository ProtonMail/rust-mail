use proton_sqlite3::rusqlite::Transaction;

pub fn create_message_tables(tx: &mut Transaction) -> crate::DBResult<()> {
    tx.execute(r#"
CREATE TABLE messages(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rid TEXT UNIQUE,
    address_id TEXT NOT NULL,
    conversation_id INTEGER NOT NULL,
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
    deleted INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT messages_address_id FOREIGN KEY (address_id) REFERENCES addresses (id),
    CONSTRAINT messageS_conversation_id FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE
)"#, ())?;

    tx.execute(
        "CREATE UNIQUE INDEX index_messages_rid ON messages (rid)",
        (),
    )?;
    tx.execute(
        "CREATE UNIQUE INDEX index_messages_cid ON messages (conversation_id)",
        (),
    )?;

    //message -> labels
    tx.execute(r#"
CREATE TABLE message_labels(
    message_id INTEGER NOT NULL,
    label_id INTEGER NOT NULL,
    PRIMARY KEY(message_id, label_id),
    CONSTRAINT message_labels_mid FOREIGN KEY (message_id) REFERENCES messages (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT message_labels_lid FOREIGN KEY (label_id) REFERENCES labels (id) ON DELETE CASCADE ON UPDATE CASCADE
)"#,())?;

    tx.execute(
        r#"
    CREATE INDEX index_messages_labels_mid ON message_labels (message_id)
"#,
        (),
    )?;
    tx.execute(
        r#"
    CREATE INDEX index_messages_labels_lid ON message_labels(label_id)
"#,
        (),
    )?;

    //messages -> attachment
    tx.execute(r#"
CREATE TABLE message_attachments(
    message_id INTEGER NOT NULL,
    attachment_id INTEGER NOT NULL,
    PRIMARY KEY(message_id, attachment_id),
    CONSTRAINT message_attachments_cid FOREIGN KEY (message_id) REFERENCES messages (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT message_attachments_aid FOREIGN KEY (attachment_id) REFERENCES attachments (id) ON DELETE CASCADE ON UPDATE CASCADE
)"#,())?;

    tx.execute(
        r#"
    CREATE INDEX index_messages_attachments_cid ON message_attachments (message_id)
"#,
        (),
    )?;
    tx.execute(
        r#"
    CREATE INDEX index_messages_attachments_aid ON message_attachments (attachment_id)
"#,
        (),
    )?;

    Ok(())
}
