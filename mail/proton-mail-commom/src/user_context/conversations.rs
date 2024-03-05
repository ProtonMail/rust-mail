use crate::{MailContextError, MailContextResult, MailUserContext};
use proton_api_mail::domain::{ConversationFilterBuilder, LabelId, MessageMetadataFilterBuilder};
use proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_api_mail::proton_api_core::exports::tracing;
use proton_api_mail::proton_api_core::exports::tracing::{debug, error, Level};
use proton_async::runtime::JoinHandle;
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
            .descending()
            .build();
        let response = session.get_conversations(filter).await?;

        {
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
        }

        //TODO: Optimize message fetching, there may be a better way.

        let chunks = response.conversations.chunks(10);
        let mut handles = Vec::with_capacity(chunks.len());
        let chunks_total = chunks.len();

        for (index, chunk) in chunks.into_iter().enumerate() {
            //TODO: scopes to share.
            let chunk = chunk.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
            let ctx = self.clone();
            let session = session.clone();
            let handle: JoinHandle<MailContextResult<()>> =
                proton_async::runtime::spawn(async move {
                    debug!(
                        "Fetching Messages Conversation chunk {}/{}",
                        index, chunks_total
                    );

                    let mut messages = Vec::new();
                    for id in chunk {
                        let filter = MessageMetadataFilterBuilder::new(0, count)
                            .with_conversation_id(id)
                            .build();
                        let messages_response = session.get_message_metadata(filter).await?;
                        messages.extend_from_slice(&messages_response.messages);
                    }

                    if !messages.is_empty() {
                        let mut connection = ctx.new_db_connection()?;
                        connection.tx(|tx| -> DBResult<()> {
                            tx.create_messages_from_metadata(messages.iter())?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                });
            handles.push(handle);
        }

        for handle in handles {
            match handle.await {
                Ok(v) => {
                    v?;
                }
                Err(e) => {
                    let e = anyhow!("Failed to join handle: {e}");
                    error!("{e}");
                    return Err(MailContextError::Other(e));
                }
            }
        }

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
