use proton_sqlite3::rusqlite::Transaction;

type RResult<T> = proton_sqlite3::rusqlite::Result<T>;

pub fn create_labels_tables(tx: &mut Transaction) -> RResult<()> {
    // Remote data version, so we can perform conflict resolution.
    tx.execute(
        r#"CREATE TABLE labels_remote (id TEXT NOT NULL PRIMARY KEY,
parent_id TEXT DEFAULT NULL,
type INTEGER NOT NULL,
`order` INTEGER NOT NULL,
name TEXT NOT NULL,
path TEXT DEFAULT NULL,
color TEXT NOT NULL,
notified INTERGER NOT NULL,
expanded INTEGER NOT NULL,
sticky INTEGER NOT NULL
)"#,
        (),
    )?;

    // Local version for manipulation.
    tx.execute(
        r#"CREATE TABLE labels (
id INTEGER PRIMARY KEY AUTOINCREMENT, rid TEXT UNIQUE, type INTEGER NOT NULL,
`order` INTEGER NOT NULL, name TEXT NOT NULL, path TEXT,
parent_id BLOB DEFAUTL NULL, color TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
notified INTERGER NOT NULL, expanded INTEGER NOT NULL, sticky INTEGER NOT NULL DEFAULT 0,
CONSTRAINT constraint_labels_rid FOREIGN KEY (rid) REFERENCES labels_remote (id) ON DELETE SET NULL,
CONSTRAINT constraint_labels_parent_id FOREIGN KEY (parent_id) REFERENCES labels (id) ON DELETE SET NULL
)"#,
        (),
    )?;

    tx.execute(
        r#"CREATE UNIQUE INDEX index_labels_rid ON labels (`rid`)"#,
        (),
    )?;
    tx.execute(r#"CREATE INDEX index_labels_order ON labels (`order`)"#, ())?;

    // Label Conversation Count
    tx.execute(r#"CREATE TABLE label_conversation_count (
label_id BLOB NOT NULL PRIMARY KEY, total INTEGER NOT NULL, unread INTEGER NOT NULL,
CONSTRAINT constraint_label_conversation_count_label_id FOREIGN KEY (label_id) REFERENCES labels (id) ON DELETE CASCADE
)"#, ())?;

    // Label Message Count
    tx.execute(r#"CREATE TABLE label_message_count (
label_id BLOB NOT NULL PRIMARY KEY, total INTEGER NOT NULL, unread INTEGER NOT NULL,
CONSTRAINT constraint_label_message_count_label_id FOREIGN KEY (label_id) REFERENCES remote_labels (id) ON DELETE CASCADE
)"#, ())?;

    const RESOLVE_PARENT_ID_TRIGGER: &str = "(SELECT id FROM labels WHERE rid=NEW.parent_id)";

    // Triggers to insert local label table from remote label
    tx.execute(&format!(
        "CREATE TRIGGER labels_insert_after_remote_insert AFTER INSERT ON labels_remote \
BEGIN \
    INSERT INTO labels (rid, parent_id, type, `order`, name, path, color, notified, expanded, sticky) \
    VALUES (NEW.id, {RESOLVE_PARENT_ID_TRIGGER}, NEW.type, \
    NEW.`order`, NEW.name, NEW.path, NEW.color, \
    NEW.notified, NEW.expanded, NEW.sticky) ON CONFLICT (rid) DO UPDATE SET \
    parent_id={RESOLVE_PARENT_ID_TRIGGER}, `order`=NEW.`order`, name=NEW.name,path=NEW.path,color=NEW.color, \
    notified=NEW.notified, expanded=NEW.expanded, sticky=NEW.sticky; \
END "),()
    )?;

    // Trigger to mark local label as deleted when remote label is deleted
    tx.execute(
        "CREATE TRIGGER labels_mark_deleted_after_remote_delete BEFORE DELETE ON labels_remote \
BEGIN \
    UPDATE labels SET deleted=2 WHERE rid=OLD.id;
END ",
        (),
    )?;

    // Triggers to update local label table from remote label
    tx.execute(
        &format!(
            "CREATE TRIGGER labels_update_after_remote_update AFTER UPDATE ON labels_remote \
BEGIN \
    UPDATE labels SET parent_id={RESOLVE_PARENT_ID_TRIGGER}, \
    `order`=NEW.`order`, name=NEW.name,path=NEW.path,color=NEW.color, \
    notified=NEW.notified, expanded=NEW.expanded, sticky=NEW.sticky WHERE rid=NEW.id; \
END "
        ),
        (),
    )?;
    Ok(())
}
