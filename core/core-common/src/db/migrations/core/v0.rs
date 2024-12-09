use proton_sqlite3::Migration;
use stash::stash::{Bond, StashError};

mod addresses;
mod contacts;
mod sender_image_cache;
mod user;
mod user_settings;

pub struct V0 {}

impl Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_v0"
    }

    async fn migrate(&self, tx: &Bond) -> Result<(), StashError> {
        addresses::create_tables(tx)?;
        user_settings::create_tables(tx)?;
        user::create_tables(tx)?;
        contacts::create_tables(tx)?;
        sender_image_cache::create_tables(tx)?;
        Ok(())
    }
}
