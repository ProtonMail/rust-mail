use crate::db::json::{deserialize_json_from_row, serde_json_err_to_sql_err};
use crate::db::{DBResult, MailSqliteConnectionImpl};
use proton_api_mail::domain::MailSettings;
use proton_api_mail::exports::serde_json;

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn create_or_update_mail_settings(&mut self, settings: &MailSettings) -> DBResult<()> {
        let settings_json = serde_json::to_string(settings).map_err(serde_json_err_to_sql_err)?;
        self.0.execute(
            "INSERT OR REPLACE INTO mail_settings VALUES(?,?)",
            (MAIL_SETTINGS_ID, settings_json),
        )?;
        Ok(())
    }

    pub fn mail_settings(&self) -> DBResult<MailSettings> {
        self.0.query_row(
            "SELECT value FROM mail_settings WHERE id=?",
            [MAIL_SETTINGS_ID],
            |r| deserialize_json_from_row(r, 0),
        )
    }
}

const MAIL_SETTINGS_ID: u32 = 1;
