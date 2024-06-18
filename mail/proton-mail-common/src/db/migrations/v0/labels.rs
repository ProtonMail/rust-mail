use stash::params;
use stash::stash::{StashError, Tether};
use proton_api_mail::domain::LabelId;

pub async fn create_labels_tables(tx: &Tether) -> Result<(), StashError> {
    // Local version for manipulation.
    tx.execute(
        r#"
            CREATE TABLE labels (
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                remote_id TEXT UNIQUE DEFAULT NULL,
                type INTEGER NOT NULL,
                `order` INTEGER NOT NULL,
                name TEXT NOT NULL,
                path TEXT DEFAULT NULL,
                parent_id BLOB DEFAULT NULL,
                color TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                notified INTEGER NOT NULL DEFAULT 0,
                expanded INTEGER NOT NULL DEFAULT 0,
                sticky INTEGER NOT NULL DEFAULT 0,
                initialized_conv INTEGER NOT NULL DEFAULT 0,
                initialized_msg INTEGER NOT NULL DEFAULT 0,

                CONSTRAINT constraint_labels_parent_id
                    FOREIGN KEY (parent_id)
                    REFERENCES labels (local_id)
                    ON DELETE SET NULL
            )
        "#,
        vec![],
    ).await?;

    tx.execute(
        r#"CREATE UNIQUE INDEX index_labels_rid ON labels (`remote_id`)"#,
        vec![],
    ).await?;
    tx.execute(r#"CREATE INDEX index_labels_order ON labels (`order`)"#, vec![]).await?;

    // Label Conversation Count
    tx.execute(
        r#"
            CREATE TABLE label_conversation_count (
                local_label_id TEXT NOT NULL PRIMARY KEY,
                total INTEGER NOT NULL,
                unread INTEGER NOT NULL,
                
                CONSTRAINT constraint_label_conversation_count_label_id
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    ).await?;

    // Label Message Count
    tx.execute(
        r#"
            CREATE TABLE label_message_count (
                local_label_id INTEGER NOT NULL PRIMARY KEY,
                total INTEGER NOT NULL,
                unread INTEGER NOT NULL,
                
                CONSTRAINT constraint_label_conversation_count_label_id
                    FOREIGN KEY (local_label_id)
                    REFERENCES labels (local_id)
                    ON DELETE CASCADE
            )
        "#,
        vec![],
    ).await?;

    // Insert default known system
    let sql =
        r#"INSERT INTO labels (remote_id, type, name, color, `order`) VALUES (?,4,?,'#000000',?)"#
    ;
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
    ];
    for (index, (id, name)) in labels.into_iter().enumerate() {
        tx.execute(sql, params![id, name, index]).await?;
    }
    Ok(())
}
