use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::db::{LocalMessageId, LocalMessageMetadata};
use proton_mail_common::proton_api_mail::domain::{MessageId, MessageMetadataFilter};
use proton_mail_common::FilteredMessages;

#[uniffi::export]
impl MailUserSession {
    /// Filter or Search messages which match the given `filter`.
    ///
    /// Note that search results are inserted into the database.
    ///
    /// # Errors
    /// Returns error if the network request or the query failed.
    pub async fn filter_messages(
        &self,
        filter: MessageMetadataFilter,
    ) -> Result<FilteredMessages, MailSessionError> {
        let ctx = self.ctx.clone();
        self.uniffi_async(async move { Ok(ctx.filter_messages(filter).await?) })
            .await
    }

    /// Retrieve the message metadata from id.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_metadata(
        &self,
        id: u64,
    ) -> Result<Option<LocalMessageMetadata>, MailSessionError> {
        Ok(self.ctx.message_metadata(LocalMessageId::from(id))?)
    }

    /// Retrieve the message metadata from `remote_id`.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn message_metadata_with_remote_id(
        &self,
        remote_id: &MessageId,
    ) -> Result<Option<LocalMessageMetadata>, MailSessionError> {
        Ok(self.ctx.message_metadata_with_remote_id(remote_id)?)
    }
}
