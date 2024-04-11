use proton_sqlite3::rusqlite::Transaction;

pub fn create_attachment_tables(tx: &mut Transaction) -> crate::db::DBResult<()> {
    // Attachments
    tx.execute(
        r#"
            CREATE TABLE attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rid TEXT UNIQUE,
                name TEXT NON NULL,
                size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                address_id TEXT DEFAULT NULL,
                key_patckets TEXT DEFAULT NULL,
                signature TEXT DEFAULT NULL,
                enc_signature TEXT DEFAULT NULL,
                disposition TEXT NOT NULL,
                
                CONSTRAINT attachments_address_id
                    FOREIGN KEY (address_id)
                    REFERENCES addresses (id)
            )
        "#,
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_attachments_rid ON attachments (rid)",
        (),
    )?;

    Ok(())
}
