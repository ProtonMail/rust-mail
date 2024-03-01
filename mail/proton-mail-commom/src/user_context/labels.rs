use crate::{MailContextResult, MailUserContext};
use proton_api_mail::domain::ALL_LABEL_TYPES;
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};
use proton_mail_db::DBResult;

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_labels(&self) -> MailContextResult<()> {
        let session = self.mail_session();

        let mut all_labels = Vec::with_capacity(64);
        for category in ALL_LABEL_TYPES {
            debug!("Fetching labels ({:?})", category);
            let labels = session.get_labels(category).await?;
            all_labels.extend(labels);
        }

        let mut connection = self.new_db_connection()?;
        debug!("Storing labels into database");
        connection.tx(|tx| -> DBResult<()> { tx.create_remote_labels(all_labels.iter()) })?;

        Ok(())
    }
}
