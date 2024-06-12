use futures::executor::block_on;
use stash::stash::{StashError, Tether};

pub fn create_tables(tx: &Tether) -> Result<(), StashError> {
    block_on(async {
    tx.execute(
        r"
        CREATE TABLE user_settings (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL,
            password TEXT NOT NULL,
            phone TEXT NOT NULL,
            two_factor_auth INTEGER NOT NULL,
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
            flags TEXT NOT NULL,
            referral TEXT,
            device_recovery INTEGER NOT NULL,
            telemetry INTEGER NOT NULL,
            crash_reports INTEGER NOT NULL,
            hide_side_panel INTEGER NOT NULL,
            high_security TEXT NOT NULL,
            session_account_recovery INTEGER NOT NULL
        )",
        vec![],
    )
    .await?;

    Ok(())
    })
}
