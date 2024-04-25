use proton_sqlite3::SqliteTransaction;

pub fn create_tables(tx: &mut SqliteTransaction) -> crate::db::DBResult<()> {
    tx.execute(
        r"
        CREATE TABLE users (
            id TEXT PRIMARY KEY,
            name TEXT,
            display_name TEXT,
            email TEXT NOT NULL,
            currency TEXT NOT NULL,
            credit INTEGER NOT NULL,
            `type` INTEGER NOT NULL,
            create_time INTEGER,
            max_space INTEGER NOT NULL,
            max_upload INTEGER NOT NULL,
            used_space INTEGER NOT NULL,
            role INTEGER NOT NULL,
            private INTEGER NOT NULL,
            to_migrate INTEGER NOT NULL,
            mnemonic_status INTEGER NOT NULL,
            subscribed INTEGER NOT NULL,
            services INTEGER NOT NULL,
            delinquent INTEGER NOT NULL,
            flags INTEGER NOT NULL,
            pus_calendar INTEGER NOT NULL DEFAULT 0,
            pus_contact INTEGER NOT NULL DEFAULT 0,
            pus_drive INTEGER NOT NULL DEFAULT 0,
            pus_mail INTEGER NOT NULL DEFAULT 0,
            pus_pass INTEGER NOT NULL DEFAULT 0
        )",
        (),
    )?;

    tx.execute(
        r"
        CREATE TABLE user_keys (
            user_id TEXT UNIQUE NOT NULL,
            key_id TEXT UNIQUE NOT NULL,
            version INTEGER NOT NULL,
            private_key TEXT NOT NULL,
            `primary` INTEGER NOT NULL,
            active INTEGER NOT NULL,
            recovery_secret TEXT,
            recovery_secret_signature TEXT,
            PRIMARY KEY(user_id, user_id)
        )",
        (),
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX index_user_keys_userid ON user_keys(user_id)",
        (),
    )?;

    Ok(())
}
