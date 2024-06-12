use crate::common::TestContext;
use proton_api_mail::domain::{ConversationId, LabelId, Message, MessageId};
use proton_api_mail::requests::{
    GetMessageResponse, LabelConversationRequest, LabelConversationsResponse,
};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    /// Generate new mock expectations for message fetch request for `message_id`.
    pub async fn mock_get_message(&self, message_id: &MessageId, message: Message) {
        let resp = GetMessageResponse { message };

        Mock::given(method("GET"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
