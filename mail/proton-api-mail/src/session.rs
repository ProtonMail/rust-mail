use crate::domain::{
    Address, ConversationFilter, ConversationId, Label, LabelId, LabelType, MailEvent, Message,
    MessageId, MessageMetadataFilter,
};
use crate::requests::{
    CreateLabelRequest, DeleteLabelRequest, GetAddressesRequest, GetConversationRequest,
    GetConversationResponse, GetConversationsRequest, GetConversationsResponse, GetLabelsRequest,
    GetMessageMetadataRequest, GetMessageRequest, MessageMetadataResponse, UpdateLabelRequest,
};
use proton_api_core::domain::EventId;
use proton_api_core::{http, Session};

/// Authenticated Session from which one can access mail related functionality
#[derive(Clone)]
pub struct MailSession {
    session: Session,
}

impl MailSession {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub async fn get_event(&self, id: &EventId) -> Result<MailEvent, http::HttpRequestError> {
        self.session.get_event::<MailEvent>(id).await
    }

    pub async fn get_labels(
        &self,
        label_type: LabelType,
    ) -> Result<Vec<Label>, http::HttpRequestError> {
        self.session
            .execute_request(GetLabelsRequest::new(label_type))
            .await
            .map(|r| r.labels)
    }

    pub async fn create_label(
        &self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&LabelId>,
    ) -> Result<Label, http::HttpRequestError> {
        self.session
            .execute_request(CreateLabelRequest::new(name, color, label_type, parent_id))
            .await
            .map(|v| v.label)
    }

    pub async fn update_label(
        &self,
        id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&LabelId>,
    ) -> Result<Label, http::HttpRequestError> {
        self.session
            .execute_request(UpdateLabelRequest::new(id, name, color, parent_id))
            .await
            .map(|v| v.label)
    }

    pub async fn delete_label(&self, parent_id: &LabelId) -> Result<(), http::HttpRequestError> {
        self.session
            .execute_request(DeleteLabelRequest::new(parent_id))
            .await
    }

    pub async fn get_message_metadata(
        &self,
        filter: MessageMetadataFilter,
    ) -> Result<MessageMetadataResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageMetadataRequest::new(filter))
            .await
    }

    pub async fn get_addresses(&self) -> Result<Vec<Address>, http::HttpRequestError> {
        self.session
            .execute_request(GetAddressesRequest {})
            .await
            .map(|v| v.addresses)
    }

    pub async fn get_message(&self, id: &MessageId) -> Result<Message, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageRequest::new(id))
            .await
            .map(|v| v.message)
    }

    pub async fn get_conversations(
        &self,
        filter: ConversationFilter,
    ) -> Result<GetConversationsResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetConversationsRequest::new(filter))
            .await
    }

    pub async fn get_conversation(
        &self,
        id: &ConversationId,
    ) -> Result<GetConversationResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetConversationRequest::new(id))
            .await
    }
}
