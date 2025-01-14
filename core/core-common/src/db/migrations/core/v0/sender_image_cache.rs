use stash::stash::{Bond, StashError};

pub async fn create_tables(tx: &Bond<'_>) -> Result<(), StashError> {
    tx.execute(
        r"
            CREATE TABLE sender_image_cache (
                local_id INTEGER PRIMARY KEY AUTOINCREMENT,
                address TEXT DEFAULT NULL,
                bimi_selector TEXT DEFAULT NULL,
                domain TEXT DEFAULT NULL,
                format TEXT DEFAULT NULL,
                max_scale_up_factor INTEGER DEFAULT NULL,
                mode INTEGER DEFAULT NULL,
                size INTEGER DEFAULT NULL,
                received_format INTEGER DEFAULT NULL,
                is_empty INTEGER NOT NULL
            )
        ",
        vec![],
    )
    .await?;

    Ok(())
}
