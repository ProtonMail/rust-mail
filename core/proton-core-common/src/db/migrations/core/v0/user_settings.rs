use proton_sqlite3::SqliteTransaction;

pub fn create_tables(tx: &mut SqliteTransaction) -> crate::db::DBResult<()> {
    tx.execute(
        r"
        CREATE TABLE user_settings (
            id TEXT PRIMARY KEY,
            email_value TEXT NOT NULL,
            email_status INTEGER NOT NULL,
            email_notify INTEGER NOT NULL,
            email_reset INTEGER NOT NULL,
            password_mode INTEGER NOT NULL,
            password_expiration_time INTEGER,
            phone_value TEXT NOT NULL,
            phone_status INTEGER NOT NULL,
            phone_notify INTEGER NOT NULL,
            phone_reset INTEGER NOT NULL,
            `2fa_enabled` INTEGER NOT NULL,
            `2fa_allowed` INTEGER NOT NULL,
            `2fa_expiration_time` INTEGER,
            news INTEGER NOT NULL,
            locale TEXT NOT NULL,
            log_auth INTEGER NOT NULL,
            invoice_text TEXT NOT NULL,
            density INTEGER NOT NULL,
            week_start INTEGER NOT NULL,
            date_format INTEGER NOT NULL,
            time_format INTEGER NOT NULL,
            welcome INTEGER NOT NULL,
            early_access INTEGER NOT NULL,
            flags_welcomed INTEGER NOT NULL,
            flags_in_app_promos_hidden INTEGER NOT NULL,
            referral_link TEXT,
            eligible INTEGER,
            device_recovery INTEGER NOT NULL,
            telemetry INTEGER NOT NULL,
            crash_reports INTEGER NOT NULL,
            hide_side_panel INTEGER NOT NULL,
            high_security_eligible INTEGER NOT NULL,
            high_security_value INTEGER NOT NULL,
            session_account_recovery INTEGER NOT NULL
        )",
        (),
    )?;

    Ok(())
}
