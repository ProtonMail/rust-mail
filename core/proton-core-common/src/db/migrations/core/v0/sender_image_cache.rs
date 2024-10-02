use futures::executor::block_on;
use stash::stash::{Interface, StashError, Tether};

pub fn create_tables(tx: &Tether) -> Result<(), StashError> {
    block_on(async {
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
    })
}
