// use crate::common::TestContext;
// use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
// use proton_api_core::services::proton::response_data::ApiErrorInfo;
// use proton_api_mail::services::proton::requests::{
//     PutConversationsLabelRequest, PutConversationsReadRequest, PutConversationsUnlabelRequest,
// };
// use proton_api_mail::services::proton::response_data::{
//     Conversation as ApiConversation, MessageMetadata, OperationResult,
// };
// use proton_api_mail::services::proton::responses::{
//     GetConversationResponse, GetConversationsResponse, PutConversationsLabelResponse,
//     PutConversationsReadResponse, PutConversationsUnlabelResponse,
// };
// use std::collections::HashSet;
// use wiremock::matchers::{body_json, method, path};
// use wiremock::{Mock, ResponseTemplate};

// impl TestContext {
//     /// Generate new mock expectations for labeling conversations.
//     ///
//     /// This function will mock the response for the given `ids` and `failed`
//     /// conversations.
//     ///
//     /// # Parameters
//     ///
//     /// * `label_id`    - The label ID to use for the request.
//     /// * `ids`         - The list of conversation IDs to label.
//     /// * `spam_action` - The spam action to use for the request.
//     /// * `failed`      - The list of conversation IDs for which we want to
//     ///                   simulate failure.
//     ///
//     pub async fn mock_label_conversation(
//         &self,
//         label_id: &ApiRemoteId,
//         ids: Vec<ApiRemoteId>,
//         spam_action: Option<bool>,
//         failed: Vec<ApiRemoteId>,
//     ) {
//         let ids = ids.into_iter().collect::<Vec<_>>();
//         let request = PutConversationsLabelRequest {
//             action: 1,
//             ids: ids.clone(),
//             label_id: label_id.clone(),
//             spam_action,
//         };
//         let resp = PutConversationsLabelResponse {
//             responses: build_conv_responses(&ids, failed),
//             undo_token: None,
//         };

//         Mock::given(method("PUT"))
//             .and(path("/api/mail/v4/conversations/label"))
//             .and(body_json(request))
//             .respond_with(ResponseTemplate::new(200).set_body_json(resp))
//             .expect(1)
//             .mount(self.mock_server())
//             .await;
//     }

//     /// Generate new mock expectations for unlabeling conversations.
//     ///
//     /// This function will mock the response for the given `ids` and `failed`
//     /// conversations.
//     ///
//     /// # Parameters
//     ///
//     /// * `label_id` - The label ID to use for the request.
//     /// * `ids`      - The list of conversation IDs to unlabel.
//     /// * `failed`   - The list of conversation IDs for which we want to
//     ///                simulate failure.
//     ///
//     pub async fn mock_unlabel_conversation(
//         &self,
//         label_id: &ApiRemoteId,
//         ids: Vec<ApiRemoteId>,
//         failed: Vec<ApiRemoteId>,
//     ) {
//         let ids = ids.into_iter().collect::<Vec<_>>();
//         let request = PutConversationsUnlabelRequest {
//             ids: ids.clone(),
//             label_id: label_id.clone(),
//         };
//         let resp = PutConversationsUnlabelResponse {
//             responses: build_conv_responses(&ids, failed),
//             undo_token: None,
//         };

//         Mock::given(method("PUT"))
//             .and(path("/api/mail/v4/conversations/unlabel"))
//             .and(body_json(request))
//             .respond_with(ResponseTemplate::new(200).set_body_json(resp))
//             .expect(1)
//             .mount(self.mock_server())
//             .await;
//     }

//     /// Generate new mock expectations for marking conversations as read.
//     ///
//     /// This function will mock the response for the given `ids` and `failed`
//     /// conversations.
//     ///
//     /// # Parameters
//     ///
//     /// * `ids`    - The list of conversation IDs to label.
//     /// * `failed` - The list of conversation IDs for which we want to
//     ///              simulate failure.
//     ///
//     pub async fn mock_mark_conversation_read(
//         &self,
//         ids: Vec<ApiRemoteId>,
//         failed: Vec<ApiRemoteId>,
//     ) {
//         let ids = ids.into_iter().collect::<Vec<_>>();
//         let request = PutConversationsReadRequest { ids: ids.clone() };
//         let resp = PutConversationsReadResponse {
//             responses: build_conv_responses(&ids, failed),
//         };

//         Mock::given(method("PUT"))
//             .and(path("/api/mail/v4/conversations/read"))
//             .and(body_json(request))
//             .respond_with(ResponseTemplate::new(200).set_body_json(resp))
//             .expect(1)
//             .mount(self.mock_server())
//             .await;
//     }

//     /// Generate new mock expectations for retrieving a `conversation` and associated `messages`'s
//     /// metadata.
//     ///
//     pub async fn mock_get_conversation(
//         &self,
//         conversation: ApiConversation,
//         messages: Vec<MessageMetadata>,
//     ) {
//         let resp = GetConversationResponse {
//             conversation,
//             messages,
//         };

//         Mock::given(method("GET"))
//             .and(path(format!(
//                 "/api/mail/v4/conversations/{}",
//                 resp.conversation.id
//             )))
//             .respond_with(ResponseTemplate::new(200).set_body_json(resp))
//             .expect(1)
//             .mount(self.mock_server())
//             .await;
//     }

//     pub async fn mock_get_image_for_conversation(&self, response: Vec<u8>) {
//         Mock::given(method("GET"))
//             .and(path("/api/core/v4/images/logo"))
//             .respond_with(ResponseTemplate::new(200).set_body_bytes(response))
//             .expect(1)
//             .mount(self.mock_server())
//             .await;
//     }
// }

// /// Build a list of conversation responses.
// ///
// /// This function builds a list of conversation responses for the given `ids`
// /// and `failed` conversations.
// ///
// /// # Parameters
// ///
// /// * `ids`    - The list of conversation IDs to build responses for.
// /// * `failed` - The list of conversation IDs for which we want to simulate
// ///              failure.
// ///
// fn build_conv_responses(ids: &[ApiRemoteId], failed: Vec<ApiRemoteId>) -> Vec<OperationResult> {
//     //TODO: ET-151
//     const CODE_SUCCESS: u32 = 1000;
//     const CODE_FAIL: u32 = 2000;

//     let failed: HashSet<ApiRemoteId> = HashSet::from_iter(failed);
//     ids.iter()
//         .map(|id| {
//             let code = if failed.contains(id) {
//                 CODE_FAIL
//             } else {
//                 CODE_SUCCESS
//             };
//             OperationResult {
//                 id: id.clone(),
//                 response: ApiErrorInfo {
//                     code,
//                     error: None,
//                     details: None,
//                 },
//             }
//         })
//         .collect()
// }
