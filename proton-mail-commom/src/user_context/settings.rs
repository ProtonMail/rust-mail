use proton_api_mail::domain::MailSettings;
use crate::exports::tracing;
use crate::exports::tracing::{debug, Level};
use crate::{MailContextResult, MailUserContext};
use proton_mail_db::DBResult;

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_mail_settings(&self) -> MailContextResult<()> {
        let session = self.mail_session();

        let settings = session.mail_settings().await?;

        let mut connection = self.new_db_connection()?;
        debug!("Storing labels into database");
        connection.tx(|tx| -> DBResult<()> { tx.create_or_update_mail_settings(&settings) })?;
        Ok(())
    }

    pub fn mail_settings(&self) -> MailContextResult<MailSettings> {
        let conn = self.new_db_connection()?;
        let settings = conn.as_connection_ref().get_mail_settings()?;
        Ok(settings)
    }
}
