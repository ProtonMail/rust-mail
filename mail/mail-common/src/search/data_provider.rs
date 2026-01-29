//! Message data provider for search indexing
//!
//! Implements the `MessageDataProvider` trait from mail-search,
//! providing access to message body and remote ID data.

use async_trait::async_trait;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_crypto_inbox::message::DecryptedBody;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_search::MessageDataProvider;
use stash::UserDb;
use stash::stash::{Stash, StashError};

use crate::datatypes::LocalMessageId as MailLocalMessageId;
use crate::models::{DraftMetadata, Message, MessageBodyMetadata, MessageMimeType, RawMessageBody};
use proton_mail_search::MessageMetadata;

/// Stash-based message data provider
///
/// Provides message data (body, remote ID) for search indexing
/// by querying the Message and MessageBody models.
#[derive(Clone)]
pub struct StashMessageDataProvider {
    stash: Stash<UserDb>,
}

impl StashMessageDataProvider {
    /// Create a new message data provider
    pub fn new(stash: Stash<UserDb>) -> Self {
        Self { stash }
    }
}

#[async_trait]
impl MessageDataProvider for StashMessageDataProvider {
    type Error = StashError;

    async fn get_body(
        &self,
        message_id: proton_mail_search::LocalMessageId,
    ) -> Result<Option<(String, bool)>, Self::Error> {
        let tether = self.stash.connection().await?;

        // Convert u64 to mail-common's LocalMessageId
        let local_id: MailLocalMessageId = message_id.into();

        // Load the raw message body and process it (similar to DecryptedMessageBody::from_raw_message_body)
        let Some(raw_body) = RawMessageBody::load(local_id, &tether).await? else {
            return Ok(None);
        };

        // Process the body to extract text content (for MIME messages, this excludes attachments)
        let processed_body = match raw_body.into_raw_decrypted_body() {
            Ok(raw_decrypted_body) => {
                match raw_decrypted_body.processed_body() {
                    Ok(decrypted_body) => {
                        // Extract the string directly from the processed body without unnecessary copy
                        match decrypted_body {
                            DecryptedBody::Plain(text) => text,
                            DecryptedBody::Mime(mime) => {
                                // For MIME, extract text content directly
                                mime.body
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to process body for message {}: {}, skipping index",
                            local_id,
                            e
                        );
                        return Ok(None);
                    }
                }
            }
            Err(_) => {
                // Decryption error case - handled here after into_raw_decrypted_body() call
                return Ok(None);
            }
        };

        // Get MIME type from metadata
        let metadata = MessageBodyMetadata::for_message(local_id, &tether).await?;
        let mime_type = metadata
            .map(|m| MessageMimeType::from_api(m.mime_type, || MessageMimeType::TextPlain))
            .unwrap_or(MessageMimeType::TextPlain);

        // Return body with is_html flag (true for TextHtml, false for TextPlain)
        let is_html = matches!(mime_type, MessageMimeType::TextHtml);
        Ok(Some((processed_body, is_html)))
    }

    async fn get_remote_id(
        &self,
        message_id: proton_mail_search::LocalMessageId,
    ) -> Result<Option<MessageId>, Self::Error> {
        let tether = self.stash.connection().await?;

        // Convert u64 to mail-common's LocalMessageId
        let local_id: MailLocalMessageId = message_id.into();

        Message::local_id_counterpart(local_id, &tether).await
    }

    async fn has_local_draft_metadata(
        &self,
        message_id: proton_mail_search::LocalMessageId,
    ) -> Result<bool, Self::Error> {
        let tether = self.stash.connection().await?;

        // Convert u64 to mail-common's LocalMessageId
        let local_id: MailLocalMessageId = message_id.into();

        // Check if there's a DraftMetadata record for this message
        // This indicates the draft is being edited locally
        let draft_metadata = DraftMetadata::find_by_message_id(local_id, &tether).await?;

        Ok(draft_metadata.is_some())
    }

    async fn get_metadata(
        &self,
        message_id: proton_mail_search::LocalMessageId,
    ) -> Result<Option<MessageMetadata>, Self::Error> {
        let tether = self.stash.connection().await?;

        // Convert u64 to mail-common's LocalMessageId
        let local_id: MailLocalMessageId = message_id.into();

        let message = Message::find_by_id(local_id, &tether).await?;

        let Some(message) = message else {
            return Ok(None);
        };

        // Extract email addresses from sender and recipients
        let from = message.sender.address.as_clear_text_str().to_string();
        let to = message
            .to_list
            .iter()
            .map(|r| r.address.as_clear_text_str())
            .collect::<Vec<_>>()
            .join(", ");
        let cc = message
            .cc_list
            .iter()
            .map(|r| r.address.as_clear_text_str())
            .collect::<Vec<_>>()
            .join(", ");
        let bcc = message
            .bcc_list
            .iter()
            .map(|r| r.address.as_clear_text_str())
            .collect::<Vec<_>>()
            .join(", ");

        Ok(Some(MessageMetadata {
            subject: message.subject,
            from,
            to,
            cc,
            bcc,
        }))
    }
}
