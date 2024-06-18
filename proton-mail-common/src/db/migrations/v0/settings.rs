use stash::stash::{StashError, Tether};

pub async fn create_settings_table(tx: &Tether) -> Result<(), StashError> {
    tx.execute(
        r#"
            CREATE TABLE mail_settings (
                id INTEGER PRIMARY KEY,
                value TEXT NOT NULL
            )
        "#,
        vec![],
    )
    .await;
    Ok(())
}
