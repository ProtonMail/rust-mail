use crate::datatypes::SystemLabelId;
use proton_core_common::datatypes::LabelId;
use stash::params;
use stash::stash::{Bond, StashError};

pub async fn create_labels_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    // Local version for manipulation.
    tx.execute(
        r#"
            CREATE TABLE labels (
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE DEFAULT NULL,
                label_type INTEGER NOT NULL,
                display INTEGER NOT NULL DEFAULT 0,
                display_order INTEGER NOT NULL,
                name TEXT NOT NULL,
                path TEXT DEFAULT NULL,
                local_parent_id INTEGER DEFAULT NULL,
                remote_parent_id TEXT DEFAULT NULL,
                color TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                notify INTEGER NOT NULL DEFAULT 0,
                expanded INTEGER NOT NULL DEFAULT 0,
                sticky INTEGER NOT NULL DEFAULT 0,
                initialized_conv INTEGER NOT NULL DEFAULT 0,
                initialized_msg INTEGER NOT NULL DEFAULT 0,
                total_conv INTEGER NOT NULL DEFAULT 0,
                total_msg INTEGER NOT NULL DEFAULT 0,
                unread_conv INTEGER NOT NULL DEFAULT 0,
                unread_msg INTEGER NOT NULL DEFAULT 0,

                CONSTRAINT constraint_labels_parent_id
                    FOREIGN KEY (local_parent_id)
                    REFERENCES labels (local_id)
                    ON DELETE SET NULL
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        r#"CREATE UNIQUE INDEX index_labels_rid ON labels (`remote_id`)"#,
        vec![],
    )
    .await?;
    tx.execute(
        r#"CREATE INDEX index_labels_order ON labels (`display_order`)"#,
        vec![],
    )
    .await?;

    // Insert default known system
    let sql = r#"INSERT INTO labels (remote_id, label_type, name, color, display_order) VALUES (?,4,?,'#000000',?)"#;
    let labels = [
        (LabelId::inbox(), "Inbox"),
        (LabelId::starred(), "Starred"),
        (LabelId::drafts(), "Drafts"),
        (LabelId::sent(), "Sent"),
        (LabelId::archive(), "Archive"),
        (LabelId::spam(), "Spam"),
        (LabelId::trash(), "Trash"),
        (LabelId::all_mail(), "All Mail"),
        (LabelId::almost_all_mail(), "Almost All Mail"),
        (LabelId::outbox(), "Outbox"),
        (LabelId::all_drafts(), "All Drafts"),
        (LabelId::all_sent(), "All Sent"),
        (LabelId::all_scheduled(), "All Scheduled"),
        (LabelId::snoozed(), "Snoozed"),
        (LabelId::category_social(), "Category Social"),
        (LabelId::category_promotions(), "Category Promotions"),
        (LabelId::category_updates(), "Category Updates"),
        (LabelId::category_forums(), "Category Forums"),
        (LabelId::category_default(), "Category Default"),
    ];
    for (index, (id, name)) in labels.into_iter().enumerate() {
        tx.execute(sql, params![id, name, index]).await?;
    }
    Ok(())
}
