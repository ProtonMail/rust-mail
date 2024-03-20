use super::MailSession;
use crate::domain::{Message, MessageCount, MessageId, MessageMetadataFilter};
use crate::requests::{
    GetMessageCountsRequest, GetMessageMetadataRequest, GetMessageRequest, MessageMetadataResponse,
};
use proton_api_core::http;

impl MailSession {
    pub async fn message(&self, id: &MessageId) -> Result<Message, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageRequest::new(id))
            .await
            .map(|v| v.message)
    }

    pub async fn message_counts(&self) -> Result<Vec<MessageCount>, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageCountsRequest {})
            .await
            .map(|r| r.counts)
    }
    pub async fn message_metadata(
        &self,
        filter: MessageMetadataFilter,
    ) -> Result<MessageMetadataResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageMetadataRequest::new(filter))
            .await
    }
}
