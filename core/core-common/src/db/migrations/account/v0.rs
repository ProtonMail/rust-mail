use stash::stash::{Bond, StashError};

pub struct V0 {}

impl proton_sqlite3::Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_db_v0"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        create_table_core_accounts(tx).await?;
        create_table_core_sessions(tx).await?;

        Ok(())
    }
}

async fn create_table_core_accounts(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r"
            CREATE TABLE core_accounts (
                -- Remote ID of the account (i.e. the API User ID)
                remote_id TEXT PRIMARY KEY,

                -- The account's username or email address (used for login)
                name_or_addr TEXT NOT NULL,

                -- Whether the account is ready (i.e. login flow completed)
                is_ready INTEGER NOT NULL,

                -- Second factor auth mode of the account
                second_factor_mode INTEGER,

                -- Mailbox password mode of the account
                password_mode INTEGER,

                -- The account's username (once known)
                username TEXT,

                -- The account's display name (once known)
                display_name TEXT,

                -- The account's primary email address (once known)
                primary_addr TEXT,

                -- Timestamp of when account was made primary
                primary_at INTEGER
            )
        ",
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE UNIQUE INDEX index_core_accounts_remote_id ON core_accounts(remote_id)",
        vec![],
    )
    .await?;

    Ok(())
}

async fn create_table_core_sessions(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r"
            CREATE TABLE core_sessions (
                -- Remote ID of the session (i.e. the API Auth UID)
                remote_id TEXT PRIMARY KEY,

                -- Account ID the session is associated with (i.e. the API User ID)
                account_id TEXT NOT NULL
                    REFERENCES core_accounts (remote_id)
                    ON DELETE CASCADE,

                -- Access token for the session
                access_token BLOB NOT NULL,

                -- Refresh token for the session
                refresh_token BLOB NOT NULL,

                -- The API scope(s) the session has access to
                auth_scopes TEXT NOT NULL,

                -- Secret used for unlocking the PGP key(s)
                key_secret BLOB
            )
        ",
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE UNIQUE INDEX index_core_sessions_remote_id ON core_sessions(remote_id)",
        vec![],
    )
    .await?;

    Ok(())
}
