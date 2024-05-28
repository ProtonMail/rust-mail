use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::proton_api_mail::domain::ConversationFilter;
use proton_mail_common::FilteredConversations;

#[uniffi::export]
impl MailUserSession {
    /// Filter or Search conversations which match the given `filter`.
    ///
    /// Note that search results are inserted into the database.
    ///
    /// # Errors
    /// Returns error if the network request or the query failed.
    pub async fn filter_conversations(
        &self,
        filter: ConversationFilter,
    ) -> Result<FilteredConversations, MailSessionError> {
        let ctx = self.ctx.clone();
        self.uniffi_async(async move { Ok(ctx.filter_conversations(filter).await?) })
            .await
    }
}
