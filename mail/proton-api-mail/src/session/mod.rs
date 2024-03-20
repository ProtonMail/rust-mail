use crate::domain::{
    Address, Label, LabelId, LabelType, MailEvent, MailSettings, Message, MessageCount, MessageId,
    MessageMetadataFilter,
};
use crate::requests::{
    CreateLabelRequest, DeleteLabelRequest, GetAddressesRequest, GetLabelsRequest,
    GetMailSettingsRequest, GetMessageCountsRequest, GetMessageMetadataRequest, GetMessageRequest,
    MessageMetadataResponse, UpdateLabelRequest,
};
use proton_api_core::domain::EventId;
use proton_api_core::{http, Session};

mod conversations;

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

    pub async fn event(&self, id: &EventId) -> Result<MailEvent, http::HttpRequestError> {
        self.session.get_event::<MailEvent>(id).await
    }

    pub async fn labels(
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

    pub async fn message_metadata(
        &self,
        filter: MessageMetadataFilter,
    ) -> Result<MessageMetadataResponse, http::HttpRequestError> {
        self.session
            .execute_request(GetMessageMetadataRequest::new(filter))
            .await
    }

    pub async fn addresses(&self) -> Result<Vec<Address>, http::HttpRequestError> {
        self.session
            .execute_request(GetAddressesRequest {})
            .await
            .map(|v| v.addresses)
    }

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

    pub async fn mail_settings(&self) -> Result<MailSettings, http::HttpRequestError> {
        self.session
            .execute_request(GetMailSettingsRequest {})
            .await
            .map(|r| r.mail_settings)
    }
}

impl From<Session> for MailSession {
    fn from(value: Session) -> Self {
        MailSession::new(value)
    }
}
