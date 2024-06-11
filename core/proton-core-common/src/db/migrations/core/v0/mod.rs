use proton_sqlite3::Migration;
use stash::stash::{StashError, Tether};

mod addresses;
mod user;
mod user_settings;
pub struct V0 {}

impl Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_v0"
    }

    fn migrate(&self, tx: &Tether) -> Result<(), StashError> {
        addresses::create_tables(tx)?;
        user_settings::create_tables(tx)?;
        user::create_tables(tx)?;
        Ok(())
    }
}
