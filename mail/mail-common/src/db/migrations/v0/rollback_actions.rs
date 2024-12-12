use stash::stash::{Bond, StashError};

pub async fn create_rollback_action_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE rollback_actions(
                remote_id TEXT NOT NULL,
                item_type INTEGER NOT NULL,
                PRIMARY KEY (remote_id, item_type)
            )
        "#,
        vec![],
    )
    .await?;

    tx.execute(
        "CREATE INDEX index_rollback_actions_item_type ON rollback_actions (item_type)",
        vec![],
    )
    .await?;

    Ok(())
}
