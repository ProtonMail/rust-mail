use crate::{MailContextResult, MailUserContext};
use proton_api_mail::proton_api_core::exports::tracing::{self, debug, Level};
use proton_mail_db::DBResult;

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_addresses(&self) -> MailContextResult<()> {
        let session = self.mail_session();

        let addresses = session.addresses().await?;

        let mut connection = self.new_db_connection()?;
        debug!("Storing labels into database");
        connection.tx(|tx| -> DBResult<()> { tx.create_or_update_addresses(addresses.iter()) })?;

        Ok(())
    }
}
