use crate::db::{DBResult, LocalConversation, LocalLabelId};
use crate::{MailContextResult, MailUserContext};
use proton_api_mail::domain::{ConversationFilterBuilder, LabelId};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn sync_first_conversation_page(
        &self,
        label_id: LabelId,
        count: usize,
    ) -> MailContextResult<()> {
        let session = self.mail_session();
        let filter = ConversationFilterBuilder::new(0, count)
            .with_label_id(label_id)
            .descending()
            .build();
        let response = session.conversations(filter).await?;

        let mut connection = self.new_db_connection()?;
        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        connection.tx(|tx| -> DBResult<()> {
            tx.create_conversations(response.conversations.iter())?;
            Ok(())
        })?;

        Ok(())
    }

    pub async fn sync_conversation_and_message_counts(&self) -> MailContextResult<()> {
        let conversation_counts = self.mail_session().conversation_counts().await?;
        let message_counts = self.mail_session().message_counts().await?;

        let mut connection = self.new_db_connection()?;
        connection.tx(|tx| -> DBResult<()> {
            tx.create_or_update_conversation_counts(conversation_counts.iter())?;
            tx.create_or_update_message_counts(message_counts.iter())?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn conversations_with_context_for_label(
        &self,
        local_label_id: LocalLabelId,
        count: usize,
    ) -> MailContextResult<Vec<LocalConversation>> {
        let connection = self.new_db_connection()?;
        Ok(connection.read(|conn| conn.get_conversations_with_context(local_label_id, count))?)
    }
}
