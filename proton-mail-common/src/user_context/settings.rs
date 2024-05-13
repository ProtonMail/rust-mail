use crate::db::DBResult;
use crate::exports::tracing;
use crate::exports::tracing::{debug, Level};
use crate::{MailContextResult, MailUserContext};
use proton_api_mail::domain::MailSettings;
use std::ops::Deref;

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

    /// Get the users mail settings.
    pub fn mail_settings(&self) -> impl Deref<Target = DBResult<MailSettings>> + '_ {
        self.inner.mail_settings.value()
    }

    /// Extract a value from the user's mail settings.
    pub fn with_mail_settings<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&DBResult<MailSettings>) -> T,
    {
        f(self.inner.mail_settings.value().deref())
    }
}
