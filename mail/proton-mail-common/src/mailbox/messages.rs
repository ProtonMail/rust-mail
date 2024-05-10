use crate::db::{LocalMessageMetadata, MessageQuery};
use crate::exports::tracing::error;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use proton_api_mail::domain::MailSettingsViewMode;

impl Mailbox {
    /// Create a new live query for messages.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Messages`].
    pub fn new_messages_query<Builder: MailboxObservableQueryBuilder<MessageQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Result<Builder::Output, MailboxError> {
        if self.view_mode() != MailSettingsViewMode::Messages {
            error!(
                "Mailbox is not in message view, current view mode = {:?}",
                self.view_mode()
            );
            return Err(MailboxError::InvalidViewMode);
        }

        Ok(builder.build(
            self.user_ctx.tracker_service().clone(),
            MessageQuery::new(self.label_id, limit),
        ))
    }

    /// Get up to `count` messages in this mailbox.
    ///
    /// # Errors
    /// Returns error if the query failed.
    pub fn messages(&self, count: usize) -> MailboxResult<Vec<LocalMessageMetadata>> {
        Ok(self
            .user_ctx
            .db_read(|conn| conn.message_metadata_list(self.label_id, count))
            .map_err(MailContextError::DB)?)
    }
}
