use proton_sqlite3::rusqlite::Transaction;

pub struct SessionMigrationV0 {}

impl proton_sqlite3::Migration for SessionMigrationV0 {
    fn name(&self) -> &str {
        "proton_core_db_v0"
    }
    fn migrate(&self, tx: &mut Transaction) -> proton_sqlite3::rusqlite::Result<()> {
        tx.execute(
            "CREATE TABLE core_sessions (id TEXT UNIQUE NOT NULL, \
user_id UNIQUE NOT NULL, email TEXT NOT NULL, name TEXT DEFAULT NULL,\
access_token BLOB NOT NULL, refresh_token BLOB NOT NULL, scopes TEXT, \
PRIMARY KEY (id, user_id))
",
            (),
        )?;

        tx.execute(
            "CREATE UNIQUE INDEX index_core_session_user_id ON core_sessions(user_id)",
            (),
        )?;
        tx.execute(
            "CREATE UNIQUE INDEX index_core_session_session_id ON core_sessions(id)",
            (),
        )?;
        Ok(())
    }
}
