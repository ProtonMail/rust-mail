//! V0 Initial db creation
mod labels;

use proton_sqlite3::rusqlite::Transaction;

pub struct MigrationV0 {}

impl proton_sqlite3::Migration for MigrationV0 {
    fn name(&self) -> &str {
        "proton_mail_db_v0"
    }

    fn migrate(&self, tx: &mut Transaction) -> proton_sqlite3::rusqlite::Result<()> {
        labels::create_labels_tables(tx)?;

        Ok(())
    }
}
