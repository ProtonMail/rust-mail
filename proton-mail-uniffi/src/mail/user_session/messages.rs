use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::proton_api_mail::domain::MessageMetadataFilter;
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
}
