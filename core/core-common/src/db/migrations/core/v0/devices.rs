use stash::stash::{Bond, StashError};

pub async fn create_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r"
        CREATE TABLE registered_devices (
            local_id INTEGER PRIMARY KEY AUTOINCREMENT,
            device_token TEXT NOT NULL,
            environment INTEGER NOT NULL,
            public_key TEXT DEFAULT NULL,
            ping_notification_status INTEGER DEFAULT NULL,
            push_notification_status INTEGER DEFAULT NULL
        )
    ",
        vec![],
    )
    .await?;

    Ok(())
}
