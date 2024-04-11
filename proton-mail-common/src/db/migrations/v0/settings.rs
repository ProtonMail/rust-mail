use proton_sqlite3::rusqlite::Transaction;
pub fn create_settings_table(tx: &mut Transaction) -> crate::db::DBResult<()> {
    tx.execute(
        r#"
            CREATE TABLE mail_settings (
                id INTEGER PRIMARY KEY,
                value TEXT NOT NULL
            )
        "#,
        (),
    )?;
    Ok(())
}
