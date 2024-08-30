use futures::executor::block_on;
use stash::stash::{Interface, StashError, Tether};

pub struct V0 {}

impl proton_sqlite3::Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_db_v0"
    }

    async fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
        block_on(async {
            tx.execute(
                r"
                CREATE TABLE core_sessions (
                	session_id TEXT UNIQUE NOT NULL,
					user_id TEXT UNIQUE NOT NULL PRIMARY KEY,
					name_or_addr TEXT NOT NULL,
					access_token BLOB NOT NULL,
					refresh_token BLOB NOT NULL,
					key_secret BLOB DEFAULT NULL,
					scopes TEXT NOT NULL DEFAULT ''
				)",
                vec![],
            )
            .await?;

            tx.execute(
                "CREATE UNIQUE INDEX index_core_session_user_id ON core_sessions(user_id)",
                vec![],
            )
            .await?;

            tx.execute(
                "CREATE UNIQUE INDEX index_core_session_session_id ON core_sessions(session_id)",
                vec![],
            )
            .await?;

            tx.execute(
                r"
                CREATE TABLE core_session_state (
                    user_id TEXT UNIQUE NOT NULL PRIMARY KEY,
                    last_active_ts INTEGER NOT NULL,

                    CONSTRAINT core_session_state_user_id
                        FOREIGN KEY (user_id)
                        REFERENCES core_sessions (user_id)
                        ON DELETE CASCADE
                )",
                vec![],
            )
            .await?;

            tx.execute(
                "CREATE UNIQUE INDEX index_core_session_state_user_id ON core_session_state(user_id)",
                vec![],
            )
            .await?;

            Ok(())
        })
    }
}
