use stash::params;
use stash::stash::{Bond, StashError};

use proton_sqlite3::Migration;

use super::v001_proton_mail_default_labels::default_labels;

pub struct MessageCountersMigration;

#[async_trait::async_trait]
impl Migration for MessageCountersMigration {
    fn name(&self) -> &str {
        "v007_proton_mail_message_counters"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        tx.execute(
            r#"
            CREATE TABLE message_counters (
                local_label_id INTEGER PRIMARY KEY,
                total INTEGER NOT NULL DEFAULT 0,
                unread INTEGER NOT NULL DEFAULT 0,

                CONSTRAINT create_message_counters_label_id
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
    "#,
            vec![],
        )
        .await?;

        // Insert message counters for default labels
        let sql = r#"INSERT INTO message_counters (local_label_id) SELECT l.local_id FROM labels AS l WHERE l.remote_id = ?"#;
        for (id, _) in default_labels().into_iter() {
            tx.execute(sql, params![id]).await?;
        }

        Ok(())
    }
}
