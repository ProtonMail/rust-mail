use stash::stash::{Interface, StashError, Tether};

pub async fn create_event_tables(tx: &Tether) -> Result<(), StashError> {
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
