use proton_sqlite3::rusqlite::Transaction;
use proton_sqlite3::Migration;

pub struct V0 {}

impl Migration for V0 {
    fn name(&self) -> &str {
        "proton_core_v0"
    }

    fn migrate(&self, tx: &mut Transaction) -> proton_sqlite3::rusqlite::Result<()> {
        tx.execute(
            "CREATE TABLE users(\
id TEXT PRIMARY KEY, name TEXT, display_name TEXT, email TEXT NOT NULL, currency TEXT NOT NULL, \
credit INTEGER NOT NULL, `type` INTEGER NOT NULL, create_time INTEGER, max_space INTEGER NOT NULL ,\
max_upload INTEGER NOT NULL, used_space INTEGER NOT NULL, \
role INTEGER NOT NULL, private INTEGER NOT NULL, to_migrate INTEGER NOT NULL,\
mnemonic_status INTEGER NOT NULL, subscribed INTEGER NOT NULL, services INTEGER NOT NULL, \
delinquent INTEGER NOT NULL, flags INTEGER NOT NULL, \
pus_calendar INTEGER NOT NULL DEFAULT 0, pus_contact INTEGER NOT NULL DEFAULT 0, \
pus_drive INTEGER NOT NULL DEFAULT 0, pus_mail INTEGER NOT NULL DEFAULT 0,
pus_pass INTEGER NOT NULL DEFAULT 0
    )",
            (),
        )?;

        tx.execute(
            "CREATE TABLE user_keys (user_id TEXT UNIQUE NOT NULL, \
key_id TEXT UNIQUE NOT NULL, version INTEGER NOT NULL, private_key TEXT NOT NULL, `primary` INTEGER NOT NULL, \
active INTEGER NOT NULL, recovery_secret TEXT , recovery_secret_signature TEXT, \
PRIMARY KEY(user_id, user_id))",
            (),
        )?;

        tx.execute(
            "CREATE UNIQUE INDEX index_user_keys_userid ON user_keys(user_id)",
            (),
        )?;

        tx.execute(
            "CREATE TABLE user_settings (
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
}
