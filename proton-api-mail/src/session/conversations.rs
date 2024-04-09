use super::MailSession;
use crate::domain::{ConversationCount, ConversationFilter, ConversationId, LabelId};
use crate::requests::{
    ConversationsResponseObject, DeleteConversationsRequest, GetConversationCountsRequest,
    GetConversationRequest, GetConversationResponse, GetConversationsRequest,
    GetConversationsResponse, LabelConversationRequest, MarkConversationsReadRequest,
    MarkConversationsUnreadRequest, UnlabelConversationRequest,
};
use proton_api_core::http;

impl MailSession {
    pub async fn conversations(
        &self,
        filter: ConversationFilter,
    ) -> Result<GetConversationsResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetConversationsRequest::new(filter))
            .await
    }

    pub async fn conversation(
        &self,
        id: &ConversationId,
    ) -> Result<GetConversationResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetConversationRequest::new(id))
            .await
    }

    pub async fn conversation_counts(
        &self,
    ) -> Result<Vec<ConversationCount>, http::HttpRequestError> {
        self.session
            .execute_request(GetConversationCountsRequest {})
            .await
            .map(|r| r.counts)
    }

    pub async fn delete_conversations(
        &self,
        label_id: &LabelId,
        ids: &[ConversationId],
    ) -> Result<Vec<ConversationsResponseObject>, http::HttpRequestError> {
        self.session
            .execute_request(DeleteConversationsRequest::new(label_id, ids))
            .await
            .map(|r| r.responses)
    }

    pub async fn mark_conversations_read(
        &self,
        ids: &[ConversationId],
    ) -> Result<Vec<ConversationsResponseObject>, http::HttpRequestError> {
        self.session
            .execute_request(MarkConversationsReadRequest::new(ids))
            .await
            .map(|r| r.responses)
    }

    pub async fn mark_conversations_unread(
        &self,
        ids: &[ConversationId],
    ) -> Result<Vec<ConversationsResponseObject>, http::HttpRequestError> {
        self.session
            .execute_request(MarkConversationsUnreadRequest::new(ids))
            .await
            .map(|r| r.responses)
    }

    pub async fn label_conversations(
        &self,
        label_id: &LabelId,
        ids: &[ConversationId],
        spam_action: Option<bool>,
    ) -> Result<Vec<ConversationsResponseObject>, http::HttpRequestError> {
        self.session
            .execute_request(LabelConversationRequest::new(label_id, spam_action, ids))
            .await
            .map(|r| r.responses)
    }

    pub async fn unlabel_conversations(
        &self,
        label_id: &LabelId,
        ids: &[ConversationId],
    ) -> Result<Vec<ConversationsResponseObject>, http::HttpRequestError> {
        self.session
            .execute_request(UnlabelConversationRequest::new(label_id, ids))
            .await
            .map(|r| r.responses)
    }
}
