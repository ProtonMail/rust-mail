use proton_api_mail::domain::LabelId;
use proton_sqlite3::SqliteTransaction;

type RResult<T> = proton_sqlite3::rusqlite::Result<T>;

pub fn create_labels_tables(tx: &mut SqliteTransaction) -> RResult<()> {
    // Local version for manipulation.
    tx.execute(
        r#"
            CREATE TABLE labels (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rid TEXT UNIQUE,
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
                    REFERENCES labels (id)
                    ON DELETE SET NULL
            )
        "#,
        (),
    )?;

    tx.execute(
        r#"CREATE UNIQUE INDEX index_labels_rid ON labels (`rid`)"#,
        (),
    )?;
    tx.execute(r#"CREATE INDEX index_labels_order ON labels (`order`)"#, ())?;

    // Label Conversation Count
    tx.execute(
        r#"
            CREATE TABLE label_conversation_count (
                label_id INTEGER NOT NULL PRIMARY KEY,
                total INTEGER NOT NULL,
                unread INTEGER NOT NULL,
                
                CONSTRAINT constraint_label_conversation_count_label_id
                    FOREIGN KEY (label_id)
                    REFERENCES labels (id)
                    ON DELETE CASCADE
            )
        "#,
        (),
    )?;

    // Label Message Count
    tx.execute(
        r#"
            CREATE TABLE label_message_count (
                label_id INTEGER NOT NULL PRIMARY KEY,
                total INTEGER NOT NULL,
                unread INTEGER NOT NULL,
                
                CONSTRAINT constraint_label_conversation_count_label_id
                    FOREIGN KEY (label_id)
                    REFERENCES labels (id)
                    ON DELETE CASCADE
            )
        "#,
        (),
    )?;

    // Insert default known system
    let mut stmt = tx.prepare(
        r#"INSERT INTO labels (rid, type, name, color, `order`) VALUES (?,4,?,'#000000',?)"#,
    )?;
    let labels = [
        (LabelId::inbox(), "Inbox"),
        (LabelId::starred(), "Starred"),
        (LabelId::drafts(), "Drafts"),
        (LabelId::sent(), "Sent"),
        (LabelId::archive(), "Archive"),
        (LabelId::spam(), "Spam"),
        (LabelId::trash(), "Trash"),
        (LabelId::all_mail(), "All Mail"),
    ];
    for (index, (id, name)) in labels.iter().enumerate() {
        stmt.execute((id, name, index))?;
    }
    Ok(())
}
