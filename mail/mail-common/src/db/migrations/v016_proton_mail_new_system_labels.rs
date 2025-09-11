//! Modifies `v001_proton_mail_default_labels`

use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use stash::params;
use stash::stash::{Bond, StashError};

use crate::datatypes::SystemLabelId;
use proton_sqlite3::Migration;
use stash::exports::SqliteError;

pub fn default_labels() -> [(LabelId, &'static str); 2] {
    [
        (LabelId::blocked(), "Blocked"),
        (LabelId::pinned(), "Pinned"),
    ]
}

pub struct DefaultLabelsMigration;

#[async_trait::async_trait]
impl Migration for DefaultLabelsMigration {
    fn name(&self) -> &str {
        "v016_proton_mail_new_system_labels"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        // Insert default known system
        let sql = r#"INSERT OR IGNORE INTO labels (remote_id, label_type, name, color, display_order) VALUES (?,4,?,'#000000',?) RETURNING local_id"#;
        let sql_message_counters = r"INSERT OR IGNORE INTO message_counters VALUES (?,0,0)";
        let sql_conversation_counters =
            r"INSERT OR IGNORE INTO conversation_counters VALUES (?,0,0)";

        for (index, (id, name)) in default_labels().into_iter().enumerate() {
            let label_id = match tx
                .query_value::<_, LocalLabelId>(sql, params![id, name, index])
                .await
            {
                Ok(id) => id,
                Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => continue,
                Err(other) => return Err(other),
            };

            tx.execute(sql_message_counters, params![label_id]).await?;
            tx.execute(sql_conversation_counters, params![label_id])
                .await?;
        }

        Ok(())
    }
}
