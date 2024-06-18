use super::MailSession;
use crate::domain::{LabelId, MessageCount, MessageId, MessageMetadataFilter};
use crate::requests::{
    DeleteMessagesRequest, DeleteMessagesResponseObject, GetMessageCountsRequest,
    GetMessageMetadataRequest, MessageMetadataResponse,
};
use proton_api_core::http;

impl MailSession {
    pub async fn message_counts(&self) -> Result<Vec<MessageCount>, http::RequestError> {
        self.session
            .execute_request(GetMessageCountsRequest {})
            .await
            .map(|r| r.counts)
    }
    pub async fn message_metadata(
        &self,
        filter: MessageMetadataFilter,
    ) -> Result<MessageMetadataResponse, http::RequestError> {
        self.session
            .execute_request(GetMessageMetadataRequest::new(filter))
            .await
    }

    pub async fn delete_messages(
        &self,
        label_id: Option<&LabelId>,
        ids: &[MessageId],
    ) -> Result<Vec<DeleteMessagesResponseObject>, http::RequestError> {
        self.session
            .execute_request(DeleteMessagesRequest::new(label_id, ids))
            .await
            .map(|r| r.responses)
    }
}
