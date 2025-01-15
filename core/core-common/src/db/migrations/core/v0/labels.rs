use stash::stash::{Bond, StashError};

pub async fn create_labels_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    // Local version for manipulation.
    tx.execute(
        r"
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

                CONSTRAINT constraint_labels_parent_id
                    FOREIGN KEY (local_parent_id)
                    REFERENCES labels (local_id)
                    ON DELETE SET NULL
            )
        ",
        vec![],
    )
    .await?;

    tx.execute(
        r"CREATE UNIQUE INDEX index_labels_rid ON labels (`remote_id`)",
        vec![],
    )
    .await?;
    tx.execute(
        r"CREATE INDEX index_labels_order ON labels (`display_order`)",
        vec![],
    )
    .await?;

    Ok(())
}
