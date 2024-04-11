use crate::db::DBResult;
use proton_sqlite3::rusqlite::Transaction;

pub fn create_event_tables(tx: &mut Transaction) -> DBResult<()> {
    tx.execute(
        r#"
            CREATE TABLE event_id_store (
                id TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        "#,
        (),
    )?;
    Ok(())
}
