use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::ApiErrorInfo;
use proton_api_mail::services::proton::requests::{
    PutMessagesLabelRequest, PutMessagesUnlabelRequest,
};
use proton_api_mail::services::proton::response_data::{
    Message as ApiMessage, MessageMetadata, OperationResult,
};
use proton_api_mail::services::proton::responses::{
    GetMessageResponse, GetMessagesResponse, PutMessagesLabelResponse, PutMessagesUnlabelResponse,
};
use proton_core_common::datatypes::RemoteId;
use std::collections::HashSet;
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
    pub async fn mock_get_messages(&self, messages: Vec<MessageMetadata>) {
        let resp = GetMessagesResponse {
            total: messages.len() as u64,
            messages,
            stale: false,
        };

        Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages".to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for labeling messages.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The label ID to use for the request.
    /// * `message_ids` - The list of message IDs to label.
    /// * `spam_action` - The spam action to use for the request.
    /// * `failed`      - The list of message IDs for which we want to
    ///                   simulate failure.
    ///
    pub async fn mock_label_messages(
        &self,
        label_id: &ApiRemoteId,
        message_ids: Vec<ApiRemoteId>,
        spam_action: Option<bool>,
        failed: Vec<ApiRemoteId>,
    ) {
        let ids = message_ids.to_vec();
        let request = PutMessagesLabelRequest {
            action: 1,
            ids: ids.clone(),
            label_id: label_id.clone(),
            spam_action,
        };
        let response = PutMessagesLabelResponse {
            responses: build_message_responses(&ids, failed),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/label"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for unlabeling messages.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The label ID to use for the request.
    /// * `message_ids` - The list of message IDs to label.
    /// * `spam_action` - The spam action to use for the request.
    /// * `failed`      - The list of message IDs for which we want to
    ///                   simulate failure.
    ///
    pub async fn mock_unlabel_messages(
        &self,
        label_id: &ApiRemoteId,
        message_ids: Vec<ApiRemoteId>,
        failed: Vec<ApiRemoteId>,
    ) {
        let ids = message_ids.to_vec();
        let request = PutMessagesUnlabelRequest {
            ids: ids.clone(),
            label_id: label_id.clone(),
        };
        let response = PutMessagesUnlabelResponse {
            responses: build_message_responses(&ids, failed),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/unlabel"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}

/// Build a list of message responses.
///
/// This function builds a list of message responses for the given `ids`
/// and `failed` messages.
///
/// # Parameters
///
/// * `ids`    - The list of message IDs to build responses for.
/// * `failed` - The list of message IDs for which we want to simulate failure.
///
fn build_message_responses(ids: &[ApiRemoteId], failed: Vec<ApiRemoteId>) -> Vec<OperationResult> {
    const CODE_SUCCESS: u32 = 1000;
    const CODE_FAIL: u32 = 2000;

    let failed: HashSet<ApiRemoteId> = HashSet::from_iter(failed);
    ids.iter()
        .map(|id| {
            let code = if failed.contains(id) {
                CODE_FAIL
            } else {
                CODE_SUCCESS
            };
            OperationResult {
                id: id.clone(),
                response: ApiErrorInfo {
                    code,
                    error: None,
                    details: None,
                },
            }
        })
        .collect()
}
