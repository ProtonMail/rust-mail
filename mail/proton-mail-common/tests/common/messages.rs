use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::{Message as ApiMessage, MessageMetadata};
use proton_api_mail::services::proton::responses::{GetMessageResponse, GetMessagesResponse};
use proton_core_common::datatypes::RemoteId;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    /// Generate new mock expectations for message fetch request for `message_id`.
    pub async fn mock_get_message(&self, message_id: &ApiRemoteId, message: ApiMessage) {
        let resp = GetMessageResponse { message };

        Mock::given(method("GET"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectation for batch messages request
    pub async fn mock_get_messages(&self, message: MessageMetadata) {
        let resp = GetMessagesResponse {
            messages: vec![message],
            stale: false,
            total: 1,
        };

        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
