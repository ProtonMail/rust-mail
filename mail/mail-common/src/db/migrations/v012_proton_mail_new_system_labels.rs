//! Modifies `v001_proton_mail_default_labels`

use proton_api_core::services::proton::LabelId;
use stash::params;
use stash::stash::{Bond, StashError};

use crate::datatypes::SystemLabelId;
use proton_sqlite3::Migration;

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
        "v012_proton_mail_new_system_labels"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        // Insert default known system
        let sql = r#"INSERT INTO labels (remote_id, label_type, name, color, display_order) VALUES (?,4,?,'#000000',?)"#;

        for (index, (id, name)) in default_labels().into_iter().enumerate() {
            tx.execute(sql, params![id, name, index]).await?;
        }

        Ok(())
    }
}
