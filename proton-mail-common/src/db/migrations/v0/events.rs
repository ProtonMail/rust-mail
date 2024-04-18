use crate::db::DBResult;
use proton_sqlite3::SqliteTransaction;

pub fn create_event_tables(tx: &mut SqliteTransaction) -> DBResult<()> {
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
