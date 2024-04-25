use proton_sqlite3::{Migration, SqliteTransaction};

mod addresses;
mod user;
mod user_settings;
pub struct V0 {}

impl Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_v0"
    }

    fn migrate(&self, tx: &mut SqliteTransaction) -> proton_sqlite3::rusqlite::Result<()> {
        addresses::create_tables(tx)?;
        user_settings::create_tables(tx)?;
        user::create_tables(tx)?;
        Ok(())
    }
}
