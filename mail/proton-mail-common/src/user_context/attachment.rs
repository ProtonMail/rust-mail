use proton_api_mail::domain::AttachmentId;
use proton_api_mail::exports::tracing;

use crate::{MailContextResult, MailUserContext};
use proton_api_mail::proton_api_core::exports::tracing::Level;

impl MailUserContext {
    /// Synchronize the full attachment metadata for the given `attachment_id`.
    ///
    /// The database might contain partial attachment metadata missing the relevant
    /// information for decryption. To synchronize the full attachment metadata this method
    /// must be called
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_complete_attachment_metadata(
        &self,
        attachment_id: AttachmentId,
    ) -> MailContextResult<()> {
        let session = self.mail_session();
        let attachment_response = session.attachment_metadata_complete(attachment_id).await?;
        self.db_write(|tx| tx.create_or_update_attachment(&attachment_response.attachment))?;
        Ok(())
    }
}
