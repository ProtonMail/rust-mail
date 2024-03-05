use crate::{MailContextResult, MailUserContext};
use proton_api_mail::domain::{ConversationFilterBuilder, LabelId};
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, Level};
use proton_mail_db::DBResult;

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
            .build();
        let conversations = session.get_conversations(filter).await?;

        let mut connection = self.new_db_connection()?;
        debug!(
            "Fetched {} conversations TOTAL={}",
            conversations.conversations.len(),
            conversations.total
        );
        connection.tx(|tx| -> DBResult<()> {
            tx.create_conversations(conversations.conversations.iter())?;
            Ok(())
        })?;

        Ok(())
    }

    pub async fn sync_conversation_and_message_counts(&self) -> MailContextResult<()> {
        let conversation_counts = self.mail_session().get_conversation_counts().await?;
        let message_counts = self.mail_session().get_message_counts().await?;

        let mut connection = self.new_db_connection()?;
        connection.tx(|tx| -> DBResult<()> {
            tx.create_or_update_conversation_counts(conversation_counts.iter())?;
            tx.create_or_update_message_counts(message_counts.iter())?;
            Ok(())
        })?;
        Ok(())
    }
}
