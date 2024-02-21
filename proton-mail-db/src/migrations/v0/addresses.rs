use proton_sqlite3::rusqlite::Transaction;

pub fn create_addresses_tables(tx: &mut Transaction) -> crate::DBResult<()> {
    // Attachments
    tx.execute(
        r#"
CREATE TABLE addresses (
    id TEXT PRIMARY KEY,
    domain_id TEXT DEFAULT NULL,
    email TEXT UNIQUE NOT NULL,
    send INTEGER NOT NULL,
    receive INTEGER NOT NULL,
    status INTEGER NOT NULL,
    type INTEGER NOT NULL,
    `order` INTEGER NOT NULL,
    display_name TEXT NOT NULL,
    signature TEXT NOT NULL,
    catch_all INTEGER NOT NULL,
    proton_mx INTEGER NOT NULL,
    signed_key_list_min_epoch_id INTEGER,
    signed_key_list_expected_min_epoch_id INTEGER,
    signed_key_list_max_epoch_id INTEGER,
    signed_key_list_data TEXT,
    signed_key_obsolescence_token TEXT,
    signed_key_signature TEXT,
    signed_key_revision INTEGER NOT NULL
)"#,
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_addresses_email ON addresses(email)",
        (),
    )?;

    tx.execute(
        r#"
CREATE TABLE address_keys (
    id TEXT PRIMARY KEY,
    address_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    private_key TEXT,
    token TEXT,
    signature TEXT,
    is_primary INTEGER NOT NULL,
    is_active INTEGER NOT NULL,
    flags INTEGER,
    address_forwarding_id TEXT,
    CONSTRAINT address_keys_id FOREIGN KEY (address_id) REFERENCES addresses (id) ON DELETE CASCADE,
    CONSTRAINT address_keys_forwarding_id FOREIGN KEY (address_forwarding_id) REFERENCES addresses (id) ON DELETE SET NULL
)"#,
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_address_keys_addr_id ON address_keys (address_id)",
        (),
    )?;

    Ok(())
}
