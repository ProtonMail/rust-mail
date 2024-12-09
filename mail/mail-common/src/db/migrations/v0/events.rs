use stash::stash::{Bond, Interface, StashError};

pub async fn create_event_tables(tx: &Bond) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE event_id_store (
                id TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        "#,
        vec![],
    )
    .await?;
    Ok(())
}
