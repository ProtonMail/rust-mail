use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::db::{LocalConversation, LocalConversationId, LocalLabelId};
use proton_mail_common::proton_api_mail::domain::{ConversationFilter, ConversationId};
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
        Ok(self.ctx.filter_conversations(filter).await?)
    }

    /// Retrieve a conversation by remote `id` in the All Mail context.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub fn conversation_with_remote_id(
        &self,
        id: &ConversationId,
    ) -> Result<Option<LocalConversation>, MailSessionError> {
        Ok(self.ctx.conversation_with_remote_id(id)?)
    }

    /// Retrieve a conversation by `id` in the `label_id` context.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub fn conversation_with_id_and_context(
        &self,
        id: u64,
        label_id: u64,
    ) -> Result<Option<LocalConversation>, MailSessionError> {
        Ok(self.ctx.conversation_with_id_and_context(
            LocalConversationId::from(id),
            LocalLabelId::from(label_id),
        )?)
    }

    /// Retrieve a conversation by `id` in the All Mail context.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub fn conversation_with_id_with_all_mail_context(
        &self,
        id: u64,
    ) -> Result<Option<LocalConversation>, MailSessionError> {
        Ok(self
            .ctx
            .conversation_with_id_with_all_mail_context(LocalConversationId::from(id))?)
    }
}
